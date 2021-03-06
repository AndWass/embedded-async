//! Single-threaded task synchronization.
//!
//! The `sync` module contains async synchronization primitives. These primitives must only
//! be used when synchronizing between tasks running on the same thread. They must not be
//! used to synchronize across threads or between an interrupt and the main application.

mod condvar;
mod mutex;

pub use condvar::*;
pub use condvar::*;
pub use mutex::*;
