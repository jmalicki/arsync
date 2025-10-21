//! Message types for GUI event handling

use std::path::PathBuf;

/// Messages handled by the main application component
#[derive(Debug, Clone)]
pub enum Message {
    /// No-op message (default for unhandled events)
    Noop,

    /// User requested to close the window
    WindowClose,

    /// User clicked "Browse Source" button
    BrowseSource,

    /// User selected a source path
    SourceSelected(PathBuf),

    /// User clicked "Browse Destination" button
    BrowseDest,

    /// User selected a destination path
    DestSelected(PathBuf),

    /// User clicked "Start Copy" button
    StartCopy,

    /// User clicked "Cancel" button
    CancelCopy,

    /// Copy operation completed
    CopyComplete(Result<(), String>),
}
