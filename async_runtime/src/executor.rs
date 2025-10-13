use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{mpsc, Arc},
    task::{Context, Poll, Waker},
};

use crate::waker::TaskWaker;

pub struct Task {
    id: usize,
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

pub struct Executor {
    tasks: HashMap<usize, Task>,
    ready_queue: mpsc::Receiver<usize>,
    waker_sender: mpsc::Sender<usize>,
    next_task_id: usize,
}

impl Executor {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Executor {
            tasks: HashMap::new(),
            ready_queue: rx,
            waker_sender: tx,
            next_task_id: 0,
        }
    }

    pub fn spawn<F, T>(&mut self, future: F) -> mpsc::Receiver<T>
    where
        F: Future<Output = T> + 'static + Send,
        T: Send + 'static,
    {
        let task_id = self.next_task_id;
        self.next_task_id += 1;

        let (tx, rx) = mpsc::channel();
        let future: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(async move {
            let result = future.await;
            let _ = tx.send(result);
        });

        let task = Task {
            id: task_id,
            future,
        };

        self.tasks.insert(task_id, task);

        let _ = self.waker_sender.send(task_id);
        rx
    }

    pub fn poll(&mut self) {
        if let Ok(task_id) = self.ready_queue.try_recv() {
            let mut task = match self.tasks.remove(&task_id) {
                Some(task) => task,
                None => return,
            };
            let waker = TaskWaker::new(task_id, self.waker_sender.clone()).into_waker();
            let mut context = Context::from_waker(&waker);

            match task.future.as_mut().poll(&mut context) {
                Poll::Ready(()) => {}
                Poll::Pending => {
                    self.tasks.insert(task_id, task);
                }
            }
        }
    }

    pub fn run(&mut self) {
        while !self.tasks.is_empty() {
            let task_id = match self.ready_queue.recv() {
                Ok(id) => id,
                Err(_) => break, // break run loop when there are no more tasks
            };

            let mut task = match self.tasks.remove(&task_id) {
                Some(task) => task,
                None => continue, // Task was already completed
            };

            let waker = TaskWaker::new(task_id, self.waker_sender.clone()).into_waker();
            let mut context = Context::from_waker(&waker);

            match task.future.as_mut().poll(&mut context) {
                Poll::Ready(()) => {}
                Poll::Pending => {
                    self.tasks.insert(task_id, task);
                }
            }
        }
    }
}
