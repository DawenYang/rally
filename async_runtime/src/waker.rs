use std::{
    sync::{mpsc::Sender, Arc},
    task::{RawWaker, RawWakerVTable, Waker},
};

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker);

#[derive(Clone)]
pub struct TaskWaker {
    task_id: usize,
    sender: Sender<usize>,
}

impl TaskWaker {
    pub fn new(task_id: usize, sender: Sender<usize>) -> Self {
        Self { task_id, sender }
    }

    pub fn into_waker(self) -> Waker {
        let arc = Arc::new(self);
        let raw = Arc::into_raw(arc);
        unsafe { Waker::from_raw(RawWaker::new(raw as *const (), &VTABLE)) }
    }
}

unsafe fn clone_waker(ptr: *const ()) -> RawWaker {
    let arc = Arc::from_raw(ptr as *const TaskWaker);
    let cloned = arc.clone();
    // Drop arc got cloned
    std::mem::forget(arc);
    let raw = Arc::into_raw(cloned);
    RawWaker::new(raw as *const (), &VTABLE)
}

unsafe fn wake(ptr: *const ()) {
    let arc = Arc::from_raw(ptr as *const TaskWaker);
    // Send task ID to wake the task
    let _ = arc.sender.send(arc.task_id);
    // Arc is dropped here, decrementing the reference count
}

unsafe fn wake_by_ref(ptr: *const ()) {
    let arc = Arc::from_raw(ptr as *const TaskWaker);
    let _ = arc.sender.send(arc.task_id);
    std::mem::forget(arc);
}

unsafe fn drop_waker(ptr: *const ()) {
    let _ = Arc::from_raw(ptr as *const TaskWaker);
    // Arc is dropped here
}
