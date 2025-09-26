use crate::runtime::FutureType;
use crate::runtime::spawn_task_function;
use crate::spawn_task;
use anyhow::{Context as _, Error, Result, bail};
use http::{Request, Uri};
use hyper_util::{
    client::legacy::connect::{Connected, Connection},
    rt::TokioIo,
};

use std::pin::Pin;
use std::task::{Context, Poll};
use std::task::Poll::Pending;
use tokio::{
    io::{self, AsyncWrite, AsyncRead},
    net::TcpStream,
    task,
};
use tokio_native_tls::TlsStream;
use tower::Service;
use hyper::rt::{Read, ReadBufCursor, Write};

pub struct Executor;

impl<F: Future + Send + 'static> hyper::rt::Executor<F> for Executor {
    fn execute(&self, fut: F) {
        spawn_task!(async {
            fut.await;
        })
        .detach();
    }
}

enum Stream {
    Plain(TokioIo<TcpStream>),
    Tls(TokioIo<TlsStream<TcpStream>>),
}

#[derive(Clone)]
struct Connector;

impl Service<Uri> for Connector {
    type Response = Stream;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let host = uri.host().expect("host is required").to_string();
        let port = uri
            .port_u16()
            .unwrap_or(if uri.scheme_str() == Some("https") {
                443
            } else {
                80
            });
        Box::pin(async move {
            let addr = format!("{}:{}", host, port);
            println!("Connecting to {}...", addr);
            let stream = TcpStream::connect(&addr)
                .await
                .context(format!("failed to connect to host: {}", addr))?;
            match uri.scheme_str() {
                Some("http") => Ok(Stream::Plain(TokioIo::new(stream))),
                Some("https") => {
                    let connector =
                        tokio_native_tls::TlsConnector::from(native_tls::TlsConnector::new()?);
                    let tls_stream = connector
                        .connect(&host, stream)
                        .await
                        .context("failed to connect TLS Stream")?;
                    Ok(Stream::Tls(TokioIo::new(tls_stream)))
                }
                scheme => bail!("unsupported scheme: {:?}", scheme),
            }
        })
    }
}

// impl Read for Stream {
//     fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: ReadBufCursor<'_>) -> Poll<io::Result<()>> {
//         // Convert hyper's ReadBufferCursor into tokio's ReadBuf
//         let slice = buf.as_mut();
//         let mut read_buf = io::ReadBuf::new(slice);
//
//         match AsyncRead::poll_read(self, cx, &mut read_buf) {
//             Poll::Ready(Ok(())) => {
//                 let n = read_buf.filled().len();
//                 buf.advance(n);
//                 Poll::Ready(Ok(()))
//             }
//             Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
//             Poll::Pending => Poll::Pending
//         }
//     }
// }
//


impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

impl Connection for Stream {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}
