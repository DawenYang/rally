use crate::reactor::{ReactorCommand, ReactorHandle};
use crate::runtime::reactor;
use mio::Token;
use std::collections::HashMap;
use std::io::{self};
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

// Global registry of wakers for stream events
lazy_static::lazy_static! {
    pub static ref STREAM_WAKERS: Mutex<HashMap<Token, Arc<Waker>>> = Mutex::new(HashMap::new());
}

/// Custom TCP stream that works with our reactor and provides tokio::io compatibility
pub struct CustomTcpStream {
    inner: tokio::net::TcpStream,
    token: Token,
    reactor: ReactorHandle,
    connected: bool,
}

impl CustomTcpStream {
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let reactor_handle = reactor();
        Self::connect_with_reactor(addr, reactor_handle).await
    }

    pub async fn connect_with_reactor<A: ToSocketAddrs>(
        addr: A,
        reactor_handle: ReactorHandle,
    ) -> io::Result<Self> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses to connect to")
        })?;

        // Use tokio's async TCP connection which handles the low-level async I/O
        let stream = tokio::net::TcpStream::connect(addr).await?;

        // Get a token from our reactor for tracking
        let token = reactor_handle
            .allocate_token()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(CustomTcpStream {
            inner: stream,
            token,
            reactor: reactor_handle,
            connected: true,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    fn register_waker(&self, waker: Waker) {
        let mut wakers = STREAM_WAKERS.lock().unwrap();
        wakers.insert(self.token, Arc::new(waker));
    }
}

// Implement tokio::io traits by delegating to the inner tokio::net::TcpStream
impl tokio::io::AsyncRead for CustomTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Register our waker with the reactor system
        self.register_waker(cx.waker().clone());

        // Delegate to tokio's implementation
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for CustomTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Register our waker with the reactor system
        self.register_waker(cx.waker().clone());

        // Delegate to tokio's implementation
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.register_waker(cx.waker().clone());
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.register_waker(cx.waker().clone());
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl Drop for CustomTcpStream {
    fn drop(&mut self) {
        // Clean up waker registration
        let mut wakers = STREAM_WAKERS.lock().unwrap();
        wakers.remove(&self.token);
        drop(wakers);

        // Notify reactor to deregister
        if let Err(e) = self
            .reactor
            .sender
            .send(ReactorCommand::Deregister(self.token))
        {
            eprintln!("Failed to deregister stream: {}", e);
        } else if let Err(e) = self.reactor.waker.wake() {
            eprintln!("Failed to wake reactor: {}", e);
        }
    }
}

// Helper function to wake stream wakers (called by reactor)
pub fn wake_stream_wakers(token: Token) {
    let wakers = STREAM_WAKERS.lock().unwrap();
    if let Some(waker) = wakers.get(&token) {
        waker.wake_by_ref();
    }
}

// Custom TLS stream that wraps our CustomTcpStream
pub struct CustomTlsStream {
    inner: CustomTcpStream,
}

impl CustomTlsStream {
    pub async fn connect(_host: &str, tcp_stream: CustomTcpStream) -> io::Result<Self> {
        // For demo purposes, just wrap the TCP stream
        // In a real implementation, this would handle TLS handshake
        Ok(CustomTlsStream { inner: tcp_stream })
    }
}

impl tokio::io::AsyncRead for CustomTlsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for CustomTlsStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
