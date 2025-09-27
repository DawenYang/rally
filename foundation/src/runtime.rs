use crate::reactor::{Reactor, ReactorHandle};
use crossbeam_channel::{Receiver, Sender};
use futures::task::{self, ArcWake};
use hyper::rt::Executor;
use num_cpus;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Context;
use std::thread;
use std::time::Duration;

thread_local! {
    static REACTOR_HANDLE: std::cell::RefCell<Option<ReactorHandle>> = std::cell::RefCell::new(None);
}

pub fn reactor() -> ReactorHandle {
    REACTOR_HANDLE.with(|handle| handle.borrow().as_ref().unwrap().clone())
}

// --- Sleep Future using futures-timer ---

pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    futures_timer::Delay::new(duration)
}

// --- Runtime startup function ---

pub async fn start_runtime() -> Runtime {
    Builder::new().build()
}

// Hyper executor implementation for our custom runtime
impl<Fut> Executor<Fut> for Spawner
where
    Fut: Future<Output = ()> + Send + 'static,
{
    fn execute(&self, fut: Fut) {
        self.spawn(fut);
    }
}

// --- Task and Spawner ---

struct Task {
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    sender: Sender<Arc<Task>>,
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let self_clone = arc_self.clone();
        arc_self
            .sender
            .send(self_clone)
            .expect("Failed to send task");
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Spawner {
    sender: Sender<Arc<Task>>,
    pub reactor_handle: ReactorHandle,
}

impl Spawner {
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let future = Box::pin(future);
        let task = Arc::new(Task {
            future: Mutex::new(future),
            sender: self.sender.clone(),
        });
        self.sender.send(task).expect("Failed to send task");
    }
}

// --- Runtime and Builder ---

#[derive(Default)]
pub struct Builder {
    threads: Option<usize>,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads);
        self
    }

    pub fn build(self) -> Runtime {
        Runtime::new(self.threads.unwrap_or_else(num_cpus::get))
    }
}

pub struct Runtime {
    _handles: Vec<thread::JoinHandle<()>>,
    spawner: Spawner,
    shutdown_tx: Sender<()>,
}

impl Runtime {
    fn new(threads: usize) -> Self {
        let (task_sender, task_receiver): (Sender<Arc<Task>>, Receiver<Arc<Task>>) =
            crossbeam_channel::unbounded();
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::unbounded();
        let (reactor_cmd_sender, reactor_cmd_receiver) = crossbeam_channel::unbounded();

        let mut reactor = Reactor::new();
        let waker = Arc::new(
            mio::Waker::new(reactor.poll_registry(), crate::reactor::WAKER_TOKEN)
                .expect("Failed to create reactor waker"),
        );
        let reactor_handle = ReactorHandle {
            sender: reactor_cmd_sender,
            waker: waker.clone(),
        };

        let shutdown_rx_clone = shutdown_rx.clone();
        let reactor_thread = thread::spawn(move || {
            reactor.run(reactor_cmd_receiver, shutdown_rx_clone);
        });

        let mut handles = Vec::new();
        for _ in 0..threads {
            let task_receiver = task_receiver.clone();
            let shutdown_rx = shutdown_rx.clone();
            let reactor_handle = reactor_handle.clone();

            handles.push(thread::spawn(move || {
                REACTOR_HANDLE.with(|handle| {
                    *handle.borrow_mut() = Some(reactor_handle);
                });

                loop {
                    crossbeam_channel::select! {
                        recv(task_receiver) -> msg => {
                            if let Ok(task) = msg {
                                let mut future = task.future.lock().unwrap();
                                let waker = task::waker(task.clone());
                                let mut context = Context::from_waker(&waker);
                                let _ = future.as_mut().poll(&mut context);
                            }
                        },
                        recv(shutdown_rx) -> _ => {
                            break;
                        }
                    }
                }
            }));
        }

        handles.push(reactor_thread);

        Self {
            _handles: handles,
            spawner: Spawner {
                sender: task_sender,
                reactor_handle,
            },
            shutdown_tx,
        }
    }

    pub fn spawner(&self) -> Spawner {
        self.spawner.clone()
    }

    pub fn shutdown(self) {
        for _ in 0..self._handles.len() {
            self.shutdown_tx.send(()).unwrap();
        }
        for handle in self._handles {
            handle.join().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{sleep, Builder};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    fn test_spawn_task() {
        let runtime = Builder::new().threads(1).build();
        let spawner = runtime.spawner();

        let flag = Arc::new(Mutex::new(false));
        let flag_clone = flag.clone();

        spawner.spawn(async move {
            *flag_clone.lock().unwrap() = true;
        });

        // Give the runtime some time to run the task
        std::thread::sleep(Duration::from_millis(100));

        assert!(*flag.lock().unwrap());
        runtime.shutdown();
    }

    #[test]
    fn test_sleep() {
        let runtime = Builder::new().threads(1).build();
        let spawner = runtime.spawner();

        let start = Instant::now();
        let duration = Duration::from_millis(200);

        spawner.spawn(async move {
            sleep(duration).await;
        });

        std::thread::sleep(duration + Duration::from_millis(100));

        assert!(start.elapsed() >= duration);
        runtime.shutdown();
    }

    #[test]
    fn test_multiple_tasks() {
        let runtime = Builder::new().threads(2).build();
        let spawner = runtime.spawner();

        let results = Arc::new(Mutex::new(Vec::new()));

        let results_clone1 = results.clone();
        spawner.spawn(async move {
            sleep(Duration::from_millis(200)).await;
            results_clone1.lock().unwrap().push(1);
        });

        let results_clone2 = results.clone();
        spawner.spawn(async move {
            sleep(Duration::from_millis(100)).await;
            results_clone2.lock().unwrap().push(2);
        });

        std::thread::sleep(Duration::from_millis(400));

        let results = results.lock().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], 2);
        assert_eq!(results[1], 1);

        runtime.shutdown();
    }
}
