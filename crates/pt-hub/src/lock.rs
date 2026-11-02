//! A mutex lock that outlives a panicking holder: the hub keeps serving with the data as the
//! holder left it.

use std::sync::{Mutex, MutexGuard, PoisonError};

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
