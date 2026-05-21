use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::error::{Error, Result};

/// Build a rayon thread pool with the given thread count, or fall back to
/// rayon's default (one thread per logical CPU).
pub fn build_pool(threads: Option<usize>) -> Result<ThreadPool> {
    let mut builder = ThreadPoolBuilder::new();
    if let Some(n) = threads {
        if n == 0 {
            return Err(Error::Io {
                path: std::path::PathBuf::new(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "thread count must be at least 1",
                ),
            });
        }
        builder = builder.num_threads(n);
    }
    builder.build().map_err(|e| Error::Io {
        path: std::path::PathBuf::new(),
        source: std::io::Error::other(e.to_string()),
    })
}

#[cfg(test)]
#[path = "parallel_tests.rs"]
mod tests;
