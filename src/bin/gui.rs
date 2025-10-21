//! GUI frontend for arsync using winio
//!
//! This provides a cross-platform graphical interface built on winio
//! (compio's official GUI framework), enabling visual file selection,
//! progress tracking, and configuration of sync options.

use winio::prelude::*;

// Re-export the main app component
use arsync::gui::ArsyncApp;

fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Run the winio application
    App::new("rs.compio.arsync").run::<ArsyncApp>(());
}
