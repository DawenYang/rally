use std::{
    future::Future,
    io::{self, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    task::Poll,
};

pub struct TcpSender {
    pub stream: Arc<Mutex<TcpStream>>,
    pub buffer: Vec<u8>,
    pub written: usize,
}

impl Future for TcpSender {
    type Output = io::Result<()>;

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

        loop {
            if self.written >= self.buffer.len() {
                return Poll::Ready(Ok(()));
            }

            match stream.write(&self.buffer[self.written..]) {
                Ok(0) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "write zero bytes",
                    )));
                }
                Ok(n) => {
                    drop(stream);
                    self.written += n;
                    // Re-acquire lock for next iteration
                    stream = match self.stream.try_lock() {
                        Ok(s) => s,
                        Err(_) => {
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                    };
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }
}
