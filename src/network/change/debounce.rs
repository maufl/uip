use std::time::{Duration, Instant};

use futures::prelude::*;

pub struct Debounce<S, I, E>
where
    S: Stream<Item = I, Error = E>,
{
    window: Duration,
    last_timestamp: Instant,
    inner: S,
}

pub fn debounce<S, I, E>(stream: S, duration: Duration) -> Debounce<S, I, E>
where
    S: Stream<Item = I, Error = E>,
{
    Debounce {
        window: duration,
        last_timestamp: Instant::now(),
        inner: stream,
    }
}

impl<S, I, E> Stream for Debounce<S, I, E>
where
    S: Stream<Item = I, Error = E>,
{
    type Item = I;
    type Error = E;

    fn poll(&mut self) -> Poll<Option<I>, E> {
        if self.last_timestamp.elapsed() > self.window {
            self.last_timestamp = Instant::now();
            self.inner.poll()
        } else {
            loop {
                match self.inner.poll() {
                    Ok(Async::Ready(_)) => {}
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => return Err(err),
                }
            }
        }
    }
}
