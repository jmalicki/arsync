//! Async synchronization primitives for compio runtime
//!
//! This crate provides async synchronization primitives that are compatible
//! with the [compio](https://github.com/compio-rs/compio) async runtime.
//!
//! # Primitives
//!
//! - [`Semaphore`] - Async semaphore for bounding concurrency
//! - [`Condvar`] - Async condition variable for task notification
//!
//! # Example
//!
//! ```rust,no_run
//! use compio_sync::Semaphore;
//! use std::sync::Arc;
//!
//! #[compio::main]
//! async fn main() {
//!     let sem = Arc::new(Semaphore::new(100));
//!     
//!     // Spawn many tasks, but only 100 run concurrently
//!     for i in 0..1000 {
//!         let sem = sem.clone();
//!         compio::runtime::spawn(async move {
//!             let _permit = sem.acquire().await;
//!             println!("Task {}", i);
//!         });
//!     }
//! }
//! ```

mod condvar;
mod semaphore;
mod waiter_queue;

pub use condvar::Condvar;
pub use semaphore::{Semaphore, SemaphorePermit};
