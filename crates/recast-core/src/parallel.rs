//! Rayon worker-pool construction for the `--threads N` flag.

use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::error::{Error, Result};

/// Build a rayon thread pool with the given thread count, or fall back to
/// rayon's default (one thread per logical CPU).
pub fn build_pool(threads: Option<usize>) -> Result<ThreadPool> {
    let mut builder = ThreadPoolBuilder::new();
    if let Some(n) = threads {
        if n == 0 {
            return Err(Error::InvalidThreads);
        }
        builder = builder.num_threads(n);
    }
    builder.build().map_err(|e| Error::ThreadPool(e.to_string()))
}

#[cfg(test)]
#[path = "parallel_tests.rs"]
mod tests;
