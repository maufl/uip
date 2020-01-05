use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug)]
pub struct Shared<T> {
    inner: Arc<RwLock<T>>,
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
            inner: Arc::new(RwLock::new(inner)),
        }
    }
    pub fn read(&self) -> RwLockReadGuard<T> {
        self.inner
            .read()
            .expect("Unable to lock shared struct for reading.")
    }

    pub fn write(&self) -> RwLockWriteGuard<T> {
        self.inner
            .write()
            .expect("Unable to lock shared struct for writing.")
    }
}
