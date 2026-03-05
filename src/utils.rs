use std::sync::Mutex;

use crate::Error;

pub(crate) fn lock_mutex<'a, T>(
    mutex: &'a Mutex<T>,
    context: &'a str,
) -> Result<std::sync::MutexGuard<'a, T>, crate::Error> {
    mutex
        .lock()
        .map_err(|e| Error::LockPoisoned(format!("{}: {}", context, e)))
}
