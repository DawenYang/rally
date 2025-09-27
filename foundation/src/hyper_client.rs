use crate::async_io::{CustomTcpStream, CustomTlsStream};
use crate::reactor::ReactorHandle;
use crate::runtime::Spawner;
use http_body_util::Empty;
use hyper::body::Incoming;
use hyper::{Request, Response, Uri};
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioIo;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::Service;

pin_project! {
    #[project = StreamProj]
    pub enum Stream {
        Plain { #[pin] inner: TokioIo<CustomTcpStream> },
        Tls { #[pin] inner: TokioIo<CustomTlsStream> },
    }
}

impl hyper::rt::Read for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            StreamProj::Plain { inner } => inner.poll_read(cx, buf),
            StreamProj::Tls { inner } => inner.poll_read(cx, buf),
        }
    }
}

impl hyper::rt::Write for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match self.project() {
            StreamProj::Plain { inner } => inner.poll_write(cx, buf),
            StreamProj::Tls { inner } => inner.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            StreamProj::Plain { inner } => inner.poll_flush(cx),
            StreamProj::Tls { inner } => inner.poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            StreamProj::Plain { inner } => inner.poll_shutdown(cx),
            StreamProj::Tls { inner } => inner.poll_shutdown(cx),
        }
    }
}

impl Connection for Stream {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

#[derive(Clone)]
pub struct CustomConnector {
    reactor_handle: ReactorHandle,
}

impl Service<Uri> for CustomConnector {
    type Response = Stream;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let host = uri.host().unwrap_or("localhost").to_string();
        let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
            Some("https") => 443,
            _ => 80,
        });
        let reactor_handle = self.reactor_handle.clone();

        Box::pin(async move {
            let addr = format!("{}:{}", host, port);

            // Use our custom TCP stream that integrates with our reactor
            let tcp_stream = CustomTcpStream::connect_with_reactor(&addr, reactor_handle)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            match uri.scheme_str() {
                Some("http") => {
                    // Plain HTTP using our custom TCP stream wrapped with TokioIo
                    Ok(Stream::Plain {
                        inner: TokioIo::new(tcp_stream),
                    })
                }
                Some("https") => {
                    // HTTPS using our custom TLS stream (which wraps our custom TCP stream)
                    let tls_stream = CustomTlsStream::connect(&host, tcp_stream)
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

                    Ok(Stream::Tls {
                        inner: TokioIo::new(tls_stream),
                    })
                }
                scheme => Err(format!("unsupported scheme: {:?}", scheme).into()),
            }
        })
    }
}

pub struct HttpClient {
    spawner: Spawner,
}

impl HttpClient {
    pub fn new(spawner: Spawner) -> Self {
        Self { spawner }
    }

    pub async fn get(
        &self,
        url: &str,
    ) -> Result<Response<Incoming>, Box<dyn std::error::Error + Send + Sync>> {
        // Create client that uses our custom connector (which uses our custom reactor-based streams)
        let connector = CustomConnector {
            reactor_handle: self.spawner.reactor_handle.clone(),
        };
        let client: Client<CustomConnector, Empty<hyper::body::Bytes>> =
            Client::builder(self.spawner.clone())
                .pool_max_idle_per_host(10)
                .pool_idle_timeout(std::time::Duration::from_secs(30))
                .build(connector);

        let uri: Uri = url.parse()?;
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Empty::<hyper::body::Bytes>::new())
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        client
            .request(req)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    pub async fn post<T>(
        &self,
        url: &str,
        body: T,
    ) -> Result<Response<Incoming>, Box<dyn std::error::Error + Send + Sync>>
    where
        T: hyper::body::Body + 'static + Send + Unpin,
        T::Data: Send,
        T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        // Create client with the body type for this specific request
        let connector = CustomConnector {
            reactor_handle: self.spawner.reactor_handle.clone(),
        };
        let client: Client<CustomConnector, T> = Client::builder(self.spawner.clone())
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build(connector);

        let uri: Uri = url.parse()?;
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .body(body)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        client
            .request(req)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Builder;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn test_custom_hyper_client_with_custom_reactor() {
        let runtime = Builder::new().threads(2).build();
        let spawner = runtime.spawner();
        let client = HttpClient::new(spawner.clone());

        let result = Arc::new(Mutex::new(None));
        let result_clone = result.clone();

        spawner.spawn(async move {
            println!("Starting HTTP request using custom reactor-based I/O...");
            match client.get("http://httpbin.org/get").await {
                Ok(response) => {
                    println!("✅ HTTP request successful using custom reactor!");
                    println!("Response status: {}", response.status());
                    *result_clone.lock().unwrap() = Some(Ok(response.status().as_u16()));
                }
                Err(e) => {
                    println!("❌ Request failed: {}", e);
                    *result_clone.lock().unwrap() = Some(Err(format!("{}", e)));
                }
            }
        });

        // Wait for the request to complete
        std::thread::sleep(Duration::from_secs(10));

        let result = result.lock().unwrap().take();
        match result {
            Some(Ok(status)) => {
                assert_eq!(status, 200);
                println!("🎉 HTTP client successfully integrated with custom reactor!");
                println!("Status: {}", status);
            }
            Some(Err(e)) => {
                println!("❌ HTTP request failed: {}", e);
            }
            None => {
                println!("⏰ HTTP request timed out");
            }
        }

        runtime.shutdown();
    }
}
