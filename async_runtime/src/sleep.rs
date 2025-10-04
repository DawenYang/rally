use std::{
    task::Poll,
    time::{Duration, Instant},
};

pub struct Sleep {
    when: Instant,
}

impl Sleep {
    pub fn new(duration: Duration) -> Self {
        Sleep {
            when: Instant::now() + duration,
        }
    }
}

impl Future for Sleep {
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let now = Instant::now();
        if now >= self.when {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
