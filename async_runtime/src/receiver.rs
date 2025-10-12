use std::{
    future::Future,
    io::{self, Read},
    net::TcpStream,
    sync::{Arc, Mutex},
    task::Poll,
};

pub struct TcpReceiver {
    pub stream: Arc<Mutex<TcpStream>>,
    pub buffer: Vec<u8>,
}

impl Future for TcpReceiver {
    type Output = io::Result<Vec<u8>>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut stream = match self.stream.try_lock() {
            Ok(stream) => stream,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        stream.set_nonblocking(true)?;
        let mut local_buf = [0; 1024];

        loop {
            match stream.read(&mut local_buf) {
                Ok(0) => {
                    // EOF - return what we have
                    return Poll::Ready(Ok(self.buffer.to_vec()));
                }
                Ok(n) => {
                    drop(stream);
                    self.buffer.extend_from_slice(&local_buf[..n]);
                    // Re-acquire lock for next iteration
                    stream = match self.stream.try_lock() {
                        Ok(s) => s,
                        Err(_) => {
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                    };
                    stream.set_nonblocking(true)?;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No more data available right now
                    if self.buffer.is_empty() {
                        // Haven't received anything yet, keep waiting
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    } else {
                        // We have some data, but there might be more coming
                        // For now, return what we have
                        return Poll::Ready(Ok(self.buffer.to_vec()));
                    }
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }
}
