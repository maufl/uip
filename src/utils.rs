use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio::sync::{Mutex,MutexGuard};

#[derive(Debug)]
pub struct Shared<T> {
    inner: Arc<Mutex<T>>,
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Shared<T> {
        Shared {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Shared<T> {
    pub fn new(inner: T) -> Shared<T> {
        Shared {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
    pub fn read(&self) -> MutexGuard<T> {
        self.inner
            .try_lock()
            .expect("Unable to lock shared struct for reading.")
    }

    pub fn write(&self) -> MutexGuard<T> {
        self.inner
            .try_lock()
            .expect("Unable to lock shared struct for writing.")
    }
}
