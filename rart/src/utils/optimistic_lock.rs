use std::cell::UnsafeCell;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot_core::SpinWait;

use crate::utils::PhantomUnsend;

pub const DEFAULT_MAX_RETRIES: u8 = 20;

#[derive(Debug, Eq, PartialEq)]
pub enum LockError {
    Locked,

    Retry,

    MaybeDeadlock(usize),
}

impl Display for LockError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Locked => write!(f, "Locked"),
            LockError::Retry => write!(f, "Retry"),
            LockError::MaybeDeadlock(num_tries) => write!(f, "MaybeDeadlock(tries: {})", num_tries),
        }
    }
}

impl std::error::Error for LockError {}

// Optimistic lock.
// Encodes a version and a lock into the same atomic.
// Reads are optimistic, but have to be retried if the version changes as a result of a write.
pub struct OptimisticLock<V> {
    // 63 bits for the version, 1 bit for the lock
    version_and_lock: AtomicU64,
    storage: UnsafeCell<V>,
    max_retries: u8,
}

impl<V> OptimisticLock<V> {
    pub fn new(storage: V) -> Self {
        Self {
            version_and_lock: AtomicU64::new(2),
            storage: UnsafeCell::new(storage),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    pub fn with_max_retries(storage: V) -> Self {
        Self {
            version_and_lock: AtomicU64::new(2),
            storage: UnsafeCell::new(storage),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    pub fn version(&self) -> u64 {
        self.version_and_lock.load(Ordering::Relaxed)
    }

    pub fn read(&self) -> Result<ReadGuard<V>, LockError> {
        ReadGuard::new(self)
    }

    pub fn read_perform<ReadResult, ReadFunction>(
        &self,
        read_function: ReadFunction,
    ) -> Result<ReadResult, LockError>
    where
        ReadFunction: Fn(&V) -> ReadResult,
    {
        let mut spin = SpinWait::new();
        let mut tries = 0;

        // Repeatedly perform operation 'f' on the value, until the version is stable (should be
        // once).
        loop {
            if tries > self.max_retries {
                return Err(LockError::MaybeDeadlock(tries as usize));
            }

            let guard = self.read()?;
            let present_value = &*guard;
            let operation_result = (read_function)(present_value);
            match guard.check_version() {
                Ok(_) => return Ok(operation_result),
                Err(LockError::Retry) => {
                    tries += 1;
                    spin.spin();
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn read_write_perform<ReadFunction, WriteFunction>(
        &mut self,
        read_function: ReadFunction,
        write_function: WriteFunction,
    ) -> Result<(), LockError>
    where
        ReadFunction: Fn(&V) -> V,
        WriteFunction: Fn(&V) -> V,
    {
        let mut spin = SpinWait::new();
        let mut tries = 0;

        // Repeatedly perform operation 'f' on the value, until the version is stable (should be
        // once).
        loop {
            if tries > self.max_retries {
                return Err(LockError::MaybeDeadlock(tries as usize));
            }

            let guard = self.read()?;
            let present_value = &*guard;
            // Perform the read.
            let operation_result = (read_function)(present_value);

            // Now perform the write function, but locking only with the exact version we
            // read. If that fails, we'll retry the whole thing.
            match self.write_with(guard.version) {
                Ok(mut g) => {
                    let v = (write_function)(&operation_result);
                    *g = v;
                    return Ok(());
                }
                Err(LockError::Retry) => {
                    tries += 1;
                    spin.spin();
                    continue;
                }
                Err(e) => return Err(e),
            };
        }
    }

    pub fn write(&self) -> Result<WriteGuard<V>, LockError> {
        let version = self.probe_lock()?;
        self.write_with(version)
    }

    pub fn write_with(&self, version: u64) -> Result<WriteGuard<V>, LockError> {
        match self.version_and_lock.compare_exchange(
            version,
            version + 0b1,
            Ordering::Acquire,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(WriteGuard::new(self)),
            Err(_) => Err(LockError::Retry),
        }
    }

    fn probe_lock(&self) -> Result<u64, LockError> {
        let version_lock = self.version_and_lock.load(Ordering::Acquire);
        if version_lock & 0x1 == 1 {
            return Err(LockError::Locked);
        }
        Ok(version_lock)
    }
}

pub struct ReadGuard<'a, V: 'a> {
    coupling: &'a OptimisticLock<V>,
    version: u64,
    _unsend_marker: PhantomUnsend,
}

impl<'a, V: 'a> ReadGuard<'a, V> {
    fn new(coupling: &'a OptimisticLock<V>) -> Result<Self, LockError> {
        let version = coupling.probe_lock()?;
        Ok(Self {
            coupling,
            version,
            _unsend_marker: Default::default(),
        })
    }
    fn check_version(self) -> Result<u64, LockError> {
        if self.version == self.coupling.probe_lock()? {
            Ok(self.version)
        } else {
            Err(LockError::Retry)
        }
    }
}

impl<'a, V> Deref for ReadGuard<'a, V> {
    type Target = V;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.coupling.storage.get() }
    }
}

pub struct WriteGuard<'a, V: 'a> {
    coupling: &'a OptimisticLock<V>,
}

impl<'a, V: 'a> WriteGuard<'a, V> {
    fn new(coupling: &'a OptimisticLock<V>) -> Self {
        Self { coupling }
    }
}

impl<'a, V: 'a> Drop for WriteGuard<'a, V> {
    fn drop(&mut self) {
        self.coupling
            .version_and_lock
            .fetch_add(1, Ordering::Release);
    }
}

impl<'a, V: 'a> Deref for WriteGuard<'a, V> {
    type Target = V;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.coupling.storage.get() }
    }
}

impl<'a, V: 'a> DerefMut for WriteGuard<'a, V> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.coupling.storage.get() }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::optimistic_lock::{LockError, OptimisticLock};

    #[test]
    fn test_read() {
        let v = OptimisticLock::new(0);
        {
            let r = v.read().unwrap();
            assert_eq!(*r, 0);
            assert!(r.check_version().is_ok());
        }
    }

    #[test]
    fn test_write_read() {
        let v = OptimisticLock::new(0);
        {
            let mut w = v.write().unwrap();
            assert_eq!(*w, 0);
            *w = 1;
        }
        {
            let r = v.read().unwrap();
            assert_eq!(*r, 1);
            assert!(r.check_version().is_ok());
        }
    }

    #[test]
    fn test_out_of_sync() {
        let v = OptimisticLock::new(0);
        {
            let r = v.read().unwrap();
            assert_eq!(*r, 0);
            {
                let mut w = v.write().unwrap();
                *w = 1;
            }
            assert_eq!(r.check_version(), Err(LockError::Retry));
        }
    }

    #[test]
    fn test_concurrent_write_with_retry() {
        static mut LOCK: Option<OptimisticLock<u64>> = None;
        unsafe { LOCK = Some(OptimisticLock::new(0)) };
        let per_thread_increments = 10000;
        let num_threads = 10;
        let do_add = move || {
            for _ in 0..per_thread_increments {
                loop {
                    unsafe {
                        match LOCK.as_ref().unwrap().write() {
                            Ok(mut guard) => {
                                *guard += 1;
                                break;
                            }
                            Err(LockError::Retry) => {
                                continue;
                            }
                            Err(LockError::Locked) => {}
                            Err(LockError::MaybeDeadlock(num_tries)) => {
                                panic!("deadlock after {}, should not happen", num_tries)
                            }
                        }
                    }
                }
            }
        };
        let add_threads = (0..num_threads)
            .map(|_| std::thread::spawn(do_add))
            .collect::<Vec<_>>();
        for t in add_threads {
            t.join().unwrap();
        }
        unsafe {
            let read = LOCK.as_ref().unwrap().read().unwrap();
            assert_eq!(*read, per_thread_increments * num_threads);
            read.check_version().unwrap();
        }
    }

    #[test]
    fn test_concurrent_write_retry_with_closure() {
        static mut LOCKS: Option<OptimisticLock<u64>> = None;
        unsafe { LOCKS = Some(OptimisticLock::new(0)) };
        let per_thread_increments = 10000;
        let num_threads = 10;
        let do_add = move || {
            for _ in 0..per_thread_increments {
                loop {
                    unsafe {
                        let op_result = LOCKS
                            .as_mut()
                            .unwrap()
                            .read_write_perform(|v| *v, |v| v + 1);
                        match op_result {
                            Ok(_) => {
                                break;
                            }
                            Err(LockError::Retry) => {
                                continue;
                            }
                            Err(LockError::Locked) => {
                                continue;
                            }
                            Err(LockError::MaybeDeadlock(num_tries)) => {
                                panic!("deadlock after {} tries, should not happen", num_tries)
                            }
                        }
                    }
                }
            }
        };
        let add_threads = (0..num_threads)
            .map(|_| std::thread::spawn(do_add))
            .collect::<Vec<_>>();
        for t in add_threads {
            t.join().unwrap();
        }
        unsafe {
            let read = LOCKS.as_ref().unwrap().read().unwrap();
            assert_eq!(*read, per_thread_increments * num_threads);
            read.check_version().unwrap();
        }
    }
}
