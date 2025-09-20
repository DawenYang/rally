use crate::runtime::FutureType;
use crate::runtime::spawn_task_function;
use crate::spawn_task;
use async_native_tls::TlsStream;
use smol::Async;
use std::net::TcpStream;

pub struct Executor;

impl<F: Future + Send + 'static> hyper::rt::Executor<F> for Executor {
    fn execute(&self, fut: F) {
        spawn_task!(async {
            fut.await;
        }).detach();
    }
}

enum Stream {
    Plain(Async<TcpStream>),
    Tls(TlsStream<Async<TcpStream>>)
}

#[derive(Clone)]
enum Connector;