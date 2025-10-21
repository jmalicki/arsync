//! GUI module for arsync
//!
//! Provides a cross-platform graphical user interface using winio.

pub mod app;
pub mod messages;

// Re-export main types
pub use app::ArsyncApp;
pub use messages::Message;
