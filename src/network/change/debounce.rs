use std::time::{Duration, Instant};
use std::task::{Context,Poll};
use std::pin::Pin;
use std::marker::Unpin;

use tokio::stream::Stream;
use pin_project_lite::pin_project;

pin_project!{
    pub struct Debounce<S> {
        window: Duration,
        last_timestamp: Instant,
        #[pin]
        inner: S,
    }
}

pub fn debounce<S, I>(stream: S, duration: Duration) -> Debounce<S>
where
    S: Stream<Item = I>,
{
    Debounce {
        window: duration,
        last_timestamp: Instant::now(),
        inner: stream,
    }
}

impl<S, I> Stream for Debounce<S>
where
    S: Stream<Item = I>,
{
    type Item = I;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<I>> {
        if self.last_timestamp.elapsed() > self.window {
            *self.as_mut().project().last_timestamp = Instant::now();
            self.project().inner.poll_next(cx)
        } else {
            loop {
                match self.as_mut().project().inner.poll_next(cx) {
                    Poll::Ready(_) => {}
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
    }
}