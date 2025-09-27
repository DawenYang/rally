use crossbeam_channel::{Receiver, Sender};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::sync::Arc;
use std::task::Waker;

// --- Reactor ---

#[allow(dead_code)]
pub enum ReactorCommand {
    Register(mio::net::TcpStream, Token, Interest, Waker),
    Deregister(Token),
    AllocateToken(crossbeam_channel::Sender<Token>),
}

#[derive(Clone)]
pub struct ReactorHandle {
    pub sender: Sender<ReactorCommand>,
    pub waker: Arc<mio::Waker>,
}

impl ReactorHandle {
    pub fn allocate_token(&self) -> Result<Token, &'static str> {
        let (token_sender, token_receiver) = crossbeam_channel::bounded(1);

        if let Err(_) = self
            .sender
            .send(ReactorCommand::AllocateToken(token_sender))
        {
            return Err("Failed to send token allocation request");
        }

        if let Err(_) = self.waker.wake() {
            return Err("Failed to wake reactor");
        }

        match token_receiver.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(token) => Ok(token),
            Err(_) => Err("Timeout waiting for token allocation"),
        }
    }
}

#[allow(dead_code)]
pub struct Reactor {
    poll: Poll,
    wakers: HashMap<Token, Waker>,
    sources: HashMap<Token, mio::net::TcpStream>,
    next_token: usize,
}

pub const WAKER_TOKEN: Token = Token(1024);

impl Reactor {
    pub fn new() -> Self {
        let poll = Poll::new().unwrap();
        Reactor {
            poll,
            wakers: HashMap::new(),
            sources: HashMap::new(),
            next_token: 1025, // Start after WAKER_TOKEN (1024)
        }
    }

    pub fn next_token(&mut self) -> Token {
        let token = Token(self.next_token);
        self.next_token += 1;
        token
    }

    pub fn run(&mut self, receiver: Receiver<ReactorCommand>, shutdown_rx: Receiver<()>) {
        println!("[Reactor] Starting reactor loop");
        loop {
            // Check for shutdown signal
            if shutdown_rx.try_recv().is_ok() {
                println!("[Reactor] Shutdown signal received, exiting reactor loop");
                break;
            }
            // Process commands
            while let Ok(command) = receiver.try_recv() {
                match command {
                    ReactorCommand::Register(mut stream, token, interest, waker) => {
                        println!("[Reactor] Registering new stream");
                        self.poll
                            .registry()
                            .register(&mut stream, token, interest)
                            .unwrap();
                        self.wakers.insert(token, waker);
                        self.sources.insert(token, stream);
                    }
                    ReactorCommand::Deregister(token) => {
                        println!("[Reactor] Deregistering stream");
                        if let Some(mut stream) = self.sources.remove(&token) {
                            self.poll.registry().deregister(&mut stream).unwrap();
                            self.wakers.remove(&token);
                        }
                    }
                    ReactorCommand::AllocateToken(response_sender) => {
                        let token = self.next_token();
                        println!("[Reactor] Allocated token: {:?}", token);
                        if let Err(e) = response_sender.send(token) {
                            eprintln!("Failed to send token: {}", e);
                        }
                    }
                }
            }

            println!("[Reactor] Polling with timeout: 100ms");
            let mut events = Events::with_capacity(1024);
            let timeout = Some(std::time::Duration::from_millis(100));
            if let Err(e) = self.poll.poll(&mut events, timeout) {
                eprintln!("[Reactor] Poll error: {}", e);
                continue;
            }

            for event in events.iter() {
                let token = event.token();
                if token == WAKER_TOKEN {
                    println!("[Reactor] Woken up by command");
                    continue;
                }
                println!("[Reactor] Got event for token: {:?}", token);
                if let Some(waker) = self.wakers.get(&token) {
                    waker.wake_by_ref();
                }

                // Also wake any streams waiting on this token
                self.wake_stream_wakers(token);
            }
        }

        // Cleanup on shutdown
        println!("[Reactor] Cleaning up resources");
        for (_token, mut stream) in self.sources.drain() {
            if let Err(e) = self.poll.registry().deregister(&mut stream) {
                eprintln!("Failed to deregister stream during cleanup: {}", e);
            }
        }
        self.wakers.clear();
        println!("[Reactor] Reactor shutdown complete");
    }

    pub fn poll_registry(&self) -> &mio::Registry {
        self.poll.registry()
    }

    fn wake_stream_wakers(&self, token: Token) {
        // Access the global stream wakers registry
        if let Ok(wakers) = crate::async_io::STREAM_WAKERS.lock() {
            if let Some(waker) = wakers.get(&token) {
                println!("[Reactor] Waking stream waker for token: {:?}", token);
                waker.wake_by_ref();
            }
        }
    }
}
