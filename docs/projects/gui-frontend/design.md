# Design: Cross-Platform GUI Frontend for arsync

**Status**: Draft → **UPDATED** (winio discovered!)  
**Author**: arsync development team  
**Created**: October 21, 2025  
**Last Updated**: October 21, 2025  
**Branch**: `gui/design-compio-frontend`  
**Implementation Branch**: `gui/impl-winio-frontend`

---

## 🎯 Executive Summary

**Decision**: Use [**winio**](https://github.com/compio-rs/winio) (compio's official GUI framework)

| Aspect | Details |
|--------|---------|
| **Framework** | winio v0.9.1 - single-threaded async GUI runtime |
| **Integration** | Perfect (built FOR compio, same thread!) |
| **Backends** | Win32, WinUI 3, GTK 4, Qt 5/6, AppKit |
| **Architecture** | Component-based (Elm), async-first |
| **Safety** | Same io_uring safety (verified, see `docs/safety/`) |
| **License** | MIT (compatible with arsync) |

**Why winio wins over iced/egui**:
- ✅ Built FOR compio → I/O + GUI in same thread (zero overhead!)
- ✅ Native backends → choose per platform (Win32/WinUI/GTK/Qt/AppKit)
- ✅ File operations built-in → FileBox, MessageBox, Progress
- ✅ `spawn()` directly → no channels, no thread boundaries

**Platform binaries**:
- Windows: `arsync-gui-win32.exe` + `arsync-gui-winui.exe`
- Linux: `arsync-gui-gtk` + `arsync-gui-qt`
- macOS: `arsync-gui.app` (AppKit)

---

## Overview

Design a cross-platform graphical user interface for arsync that provides:
- Visual source/destination selection (local and remote via SSH)
- All CLI options with contextual help
- Real-time progress visualization
- Native look-and-feel on each platform (via winio backends)
- **Direct compio integration** (same thread, zero overhead!)

This will make arsync accessible to users who prefer GUI over CLI while maintaining the same high-performance io_uring backend.

---

## Problem Statement

### Current Situation

arsync is a powerful, high-performance file synchronization tool but:
- **CLI-only**: No graphical interface
- **Experts only**: Requires command-line knowledge
- **Limited discoverability**: Users don't know all options exist
- **No visual feedback**: Progress only via terminal output
- **Manual path entry**: No file/folder picker dialogs

### Challenges

1. **Cross-platform UI**: Windows, macOS, Linux with native feel
2. **Async integration**: GUI must work with compio's completion-based I/O
3. **Framework maturity**: Need stable, production-ready GUI framework
4. **Performance**: GUI shouldn't slow down io_uring operations
5. **Remote support**: Handle SSH connections visually
6. **Real-time updates**: Show progress without blocking I/O

### Goals

- ✅ Cross-platform (Windows, macOS, Linux)
- ✅ Native look-and-feel (or polished custom widgets)
- ✅ File/folder picker dialogs (local and remote)
- ✅ All CLI options accessible with help tooltips
- ✅ Real-time progress visualization
- ✅ Works with compio async runtime
- ✅ Professional appearance
- ✅ Responsive UI (doesn't block on I/O)

### Non-Goals

- ❌ Mobile support (iOS/Android)
- ❌ Web-based UI (desktop-only for now)
- ❌ Plugin system (keep it simple)
- ❌ Multi-language i18n (English-first, add later if needed)

---

## Critical Discovery: winio - compio's Official GUI Framework!

### Research Findings ✅

**FOUND IT!** compio has an official GUI framework: [**winio**](https://github.com/compio-rs/winio)

**What is winio**:
- ✅ **Single-threaded async GUI runtime** based on compio
- ✅ **Native backends** for all platforms:
  - Windows: Win32, WinUI 3
  - Linux: GTK 4, Qt 5/6
  - macOS: AppKit
- ✅ **Elm-like architecture** (like iced, yew, relm4)
- ✅ **Async-first**: All I/O in same thread as GUI without blocking!
- ✅ **compio integration**: Built specifically for compio runtime
- ✅ **MIT licensed**: Same as arsync
- ✅ **Active**: Latest release v0.9.1 (Oct 18, 2025)

**Key Features from examples**:
- File/folder pickers (`FileBox`)
- Message boxes
- Canvas for custom drawing
- Buttons, labels, text boxes
- Tab views, scroll views
- Progress bars, sliders
- Web views (optional feature)
- Media playback (optional feature)

**This changes EVERYTHING** - we should use winio, not iced!

---

## GUI Framework Options

### Option 1: winio (✅ RECOMMENDED - compio's official GUI)

**What it is**: compio's official single-threaded async GUI runtime with native backends

**Repository**: https://github.com/compio-rs/winio  
**Status**: Active (v0.9.1, Oct 2025)

**Pros**:
- ✅ **Built FOR compio**: Perfect integration, same thread for I/O + GUI
- ✅ **Multiple native backends**: Choose per platform
  - Windows: Win32 (classic) or WinUI 3 (modern)
  - Linux: GTK 4 or Qt 5/6
  - macOS: AppKit (native)
- ✅ **Elm architecture**: Same as iced/yew (message passing, reactive)
- ✅ **Async-first**: `async fn update()`, `spawn()` for I/O
- ✅ **No blocking**: I/O in same thread as GUI without blocking UI!
- ✅ **Rich widgets**: Buttons, labels, text boxes, file pickers, progress bars
- ✅ **File dialogs**: Native `FileBox` for file/folder selection
- ✅ **Message boxes**: Built-in alert/confirm dialogs
- ✅ **Layouts**: Grid, StackPanel, custom layouts
- ✅ **Canvas**: Custom drawing support
- ✅ **MIT licensed**: Compatible with arsync
- ✅ **Same safety model**: Verified safe (see `docs/safety/`)

**Cons**:
- ⚠️ Young project (v0.9.x)
- ⚠️ Less ecosystem than iced
- ⚠️ Fewer third-party widgets
- ⚠️ Documentation still growing

**Example from winio**:
```rust
use compio::{fs::File, io::AsyncReadAtExt, runtime::spawn};
use winio::prelude::*;

impl Component for FsPage {
    async fn update(&mut self, message: Message, sender: &ComponentSender<Self>) -> bool {
        match message {
            Message::OpenFile(path) => {
                // Spawn compio I/O operation directly!
                spawn(async move {
                    let file = File::open(path).await?;
                    let (_, buffer) = file.read_to_end_at(vec![], 0).await;
                    // Process buffer...
                }).detach();
                true
            }
        }
    }
}
```

**Integration**:
- Zero overhead - compio I/O runs in same thread!
- No thread boundaries, no channels needed
- Just spawn() async operations
- Perfect for arsync

**Verdict**: ✅ **BEST CHOICE** - Official compio GUI, perfect integration!

---

### Option 2: iced (Alternative if winio immature)

**What it is**: Rust-native, cross-platform GUI framework inspired by Elm architecture

**Pros**:
- ✅ **Runtime agnostic**: Works with any async runtime (tokio, async-std, **compio**)
- ✅ **Cross-platform**: Windows, macOS, Linux with native widgets
- ✅ **Reactive architecture**: Elm-like message passing (good for async)
- ✅ **Active development**: Well-maintained, growing ecosystem
- ✅ **Native file dialogs**: `rfd` crate integration
- ✅ **Async-first**: Designed for async operations
- ✅ **Beautiful**: Modern, polished widgets
- ✅ **Type-safe**: Strong Rust types throughout

**Cons**:
- ⚠️ Some APIs still evolving (0.13 as of 2025)
- ⚠️ Learning curve (Elm architecture)
- ⚠️ Custom styling takes effort

**Integration with compio**:
```rust
// iced runs on its own event loop
// Spawn compio operations and send results via messages
use iced::{Application, Command};

impl Application for ArsyncGui {
    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::StartCopy => {
                // Spawn compio task
                Command::perform(
                    async {
                        compio::task::spawn(copy_operation()).await
                    },
                    Message::CopyProgress
                )
            }
            // ...
        }
    }
}
```

**Verdict**: ✅ **Best choice** - mature, cross-platform, async-friendly

---

### Option 2: egui (Immediate Mode)

**What it is**: Immediate-mode GUI framework (like ImGui)

**Pros**:
- ✅ Simple API (immediate mode)
- ✅ Cross-platform
- ✅ Fast iteration
- ✅ Great for tools/utilities
- ✅ Runtime agnostic

**Cons**:
- ⚠️ Not native widgets (custom rendering)
- ⚠️ Doesn't feel "native" on any platform
- ⚠️ File dialogs require extra crates
- ⚠️ Immediate mode harder for complex state

**Integration**:
```rust
// egui redraws every frame - need to manage state carefully
compio::runtime::spawn(async {
    loop {
        egui_frame.update(|ctx| {
            // UI code
        });
    }
});
```

**Verdict**: ⚠️ **Second choice** - good for prototypes, less polished for end users

---

### Option 3: Slint

**What it is**: Native GUI toolkit with declarative markup language

**Pros**:
- ✅ Native widgets
- ✅ Declarative UI (like QML)
- ✅ Cross-platform
- ✅ Professional appearance
- ✅ Runtime agnostic

**Cons**:
- ⚠️ Extra build complexity (markup compiler)
- ⚠️ Smaller ecosystem than iced
- ⚠️ Learning curve for markup language

**Verdict**: ⚠️ **Alternative** - good but more complex build

---

### Option 4: Tauri (Web Tech)

**What it is**: Desktop apps using web technologies (HTML/CSS/JS) with Rust backend

**Pros**:
- ✅ Use web technologies (React, Vue, etc.)
- ✅ Cross-platform
- ✅ Huge ecosystem (npm packages)
- ✅ Easy for web developers

**Cons**:
- ❌ Large bundle size
- ❌ Web tech dependency (not pure Rust)
- ❌ Overhead of web runtime
- ❌ Doesn't fit "native" goal

**Verdict**: ❌ **Not recommended** - too heavy for file sync tool

---

### Option 5: Qt Bindings (qt_widgets / cxx-qt)

**What it is**: Rust bindings to Qt framework

**Pros**:
- ✅ Mature Qt framework
- ✅ Native widgets
- ✅ Extensive features
- ✅ Professional appearance

**Cons**:
- ❌ Qt dependency (large, complex)
- ❌ Build complexity (C++ toolchain required)
- ❌ License concerns (GPL/LGPL/Commercial)
- ❌ Bindings less mature than Qt itself
- ❌ Not idiomatic Rust

**Verdict**: ❌ **Not recommended** - too complex, licensing issues

---

## Windows Backend Choice (winio-specific)

With winio, we get to **choose the Windows backend** at compile time!

### Option A: Win32 (winio-ui-win32)

**What it is**: Classic Windows native widgets via Win32 API

**Pros**:
- ✅ Works on all Windows versions (7+)
- ✅ Lightweight
- ✅ Classic Windows look
- ✅ No extra dependencies
- ✅ Fast startup

**Cons**:
- ⚠️ Older look (classic Windows theme)
- ⚠️ Less modern features

**Use case**: Maximum compatibility, lightweight

---

### Option B: WinUI 3 (winio-ui-winui)

**What it is**: Modern Windows UI framework (successor to UWP)

**Pros**:
- ✅ Modern Windows 11 look (Fluent Design)
- ✅ Native dark/light theme
- ✅ Beautiful, polished widgets
- ✅ Modern features (acrylic, mica backgrounds)
- ✅ Future of Windows UI

**Cons**:
- ⚠️ Requires Windows 10 1809+ (WinUI 3 runtime)
- ⚠️ Larger dependency
- ⚠️ Slower startup

**Use case**: Modern, beautiful Windows app

---

### Recommended: **Dual Build** 🎯

**Ship BOTH variants**:
- `arsync-gui-win32.exe` - Classic, compatible
- `arsync-gui-winui.exe` - Modern, beautiful

Users choose based on preference/Windows version!

**Build configuration**:
```toml
[[bin]]
name = "arsync-gui-win32"
path = "src/bin/gui.rs"
required-features = ["gui-win32"]

[[bin]]
name = "arsync-gui-winui"  
path = "src/bin/gui.rs"
required-features = ["gui-winui"]
```

---

### Linux Backend Choice

**Option A: GTK 4** (Recommended for GNOME/most distros)
- Native look on GNOME-based systems
- Modern, well-supported

**Option B: Qt 5/6** (Recommended for KDE/Qt-based systems)
- Native look on KDE
- More features, heavier

**Default**: GTK 4 (wider compatibility)

---

## Recommended Approach

### Primary Recommendation: **winio** 🎯

**Why**:
1. ✅ **Built FOR compio** - perfect integration, same runtime!
2. ✅ **Single-threaded** - I/O + GUI in same thread (no overhead!)
3. ✅ **Native backends** - Win32/WinUI3/GTK4/Qt/AppKit
4. ✅ **Async-first** - `async fn update()`, spawn() directly
5. ✅ **File operations built-in** - FileBox, MessageBox
6. ✅ **Elm architecture** - same reactive model as iced
7. ✅ **Same safety guarantees** - verified io_uring safety
8. ✅ **MIT licensed** - compatible with arsync

**Architecture** (winio = compio + GUI):
```
┌────────────────────────────────────────┐
│      winio GUI Framework               │
│  (Event loop, native widgets, canvas)  │
│                                        │
│  Backends:                             │
│  - Windows: Win32 / WinUI 3           │
│  - Linux:   GTK 4 / Qt 5/6            │
│  - macOS:   AppKit                     │
└──────────────┬─────────────────────────┘
               │
               │ Component messages (Elm)
               │
┌──────────────▼─────────────────────────┐
│       arsync GUI Application           │
│  (Component, update logic, render)     │
│                                        │
│  async fn update() {                   │
│    spawn(compio_op).detach()          │
│  }                                     │
└──────────────┬─────────────────────────┘
               │
               │ SAME THREAD!
               │ (spawn = compio::runtime::spawn)
               │
┌──────────────▼─────────────────────────┐
│        compio Runtime (built-in)       │
│  (io_uring, async I/O, file ops)       │
│  Already integrated in winio!          │
└────────────────────────────────────────┘
```

**Communication pattern** (simpler than iced!):
- User clicks button → `Component::update()` called
- `spawn(async { File::open(...).await })` - compio I/O in same thread!
- I/O completes → `sender.post(Message::Complete(data))`
- `update()` called again → State updated, GUI re-renders
- **No channels, no thread boundaries, no overhead!**

---

## Proposed Solution

### High-Level Approach

**Single-tier architecture** (winio = GUI + compio):

**Component-based design**:
- `MainComponent` - Root window, tab view, menu
- `SourcePanel` - Source selection (local/remote)
- `DestPanel` - Destination selection (local/remote)
- `OptionsPanel` - Sync options (preserve metadata, recursive, etc.)
- `ProgressView` - Real-time progress, stats
- `LogPanel` - Operation log, errors

**All in ONE thread**:
- GUI events → `async fn update()`
- File I/O → `spawn(compio_op)` - same thread!
- Progress → Direct state updates
- No channels, no overhead!

**Integration with existing arsync**:
- Reuse all existing arsync code (`src/copy.rs`, `src/sync.rs`, etc.)
- Just add GUI layer on top
- Call existing functions from `spawn()` blocks
- Progress callbacks → `sender.post(Message::Progress(...))`

---

## Architecture

### Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                       iced Application                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐  │
│  │  Source      │  │ Destination  │  │   Options       │  │
│  │  Panel       │  │  Panel       │  │   Panel         │  │
│  │              │  │              │  │                 │  │
│  │ [Browse...]  │  │ [Browse...]  │  │ ☑ Preserve      │  │
│  │ /home/user   │  │ /backup      │  │ ☑ Recursive     │  │
│  │              │  │              │  │ Buffer: 64KB    │  │
│  └──────────────┘  └──────────────┘  └─────────────────┘  │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐ │
│  │              Progress Visualization                    │ │
│  │  ████████████████░░░░░░░░░░░░░░  60% (1.2GB/2.0GB)   │ │
│  │  Speed: 250 MB/s  │  ETA: 3s  │  Files: 42/100      │ │
│  └───────────────────────────────────────────────────────┘ │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐                        │
│  │   [Cancel]   │  │  [Start Copy]│                        │
│  └──────────────┘  └──────────────┘                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ Commands (Elm messages)
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Application State                         │
├─────────────────────────────────────────────────────────────┤
│  • source_path: PathBuf                                     │
│  • dest_path: PathBuf                                       │
│  • options: CopyOptions                                     │
│  • progress: Option<ProgressState>                          │
│  • status: Idle | Running | Complete | Error                │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ spawn_local / channels
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                  arsync Core (compio)                        │
├─────────────────────────────────────────────────────────────┤
│  • copy_file() / copy_directory()                           │
│  • Progress tracking via callbacks                          │
│  • io_uring operations                                      │
│  • All existing functionality                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Key Components

### Component 1: Main Application (iced::Application)

**Purpose**: Main GUI application structure

**Location**: `src/gui/mod.rs`

**Key Types**:
```rust
pub struct ArsyncGui {
    source_path: PathBuf,
    dest_path: PathBuf,
    options: CopyOptions,
    progress: Option<ProgressState>,
    status: AppStatus,
}

pub enum Message {
    SourceBrowse,
    SourceSelected(PathBuf),
    DestBrowse,
    DestSelected(PathBuf),
    OptionChanged(OptionType),
    StartCopy,
    CancelCopy,
    ProgressUpdate(ProgressInfo),
    CopyComplete(Result<Stats>),
    Error(String),
}
```

**Responsibilities**:
- Handle user input
- Manage application state
- Spawn async copy operations
- Receive and display progress updates

---

### Component 2: File Browser Dialog

**Purpose**: Native file/folder picker

**Location**: `src/gui/dialogs.rs`

**Implementation**: Use `rfd` (Rust File Dialog) crate
- Native dialogs on all platforms
- Async-compatible
- Folder and file selection

```rust
use rfd::AsyncFileDialog;

async fn browse_folder() -> Option<PathBuf> {
    AsyncFileDialog::new()
        .set_title("Select Source Folder")
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
}
```

---

### Component 3: Remote Path Selector

**Purpose**: SSH host/path selection

**Location**: `src/gui/remote.rs`

**UI**:
```
┌──────────────────────────────────────┐
│  Remote Connection                   │
├──────────────────────────────────────┤
│  Host: [user@hostname.com    ]       │
│  Port: [22           ]               │
│  Path: [/remote/path         ]       │
│                                      │
│  ☑ Use SSH Agent                     │
│  ☐ Password: [***********]           │
│                                      │
│  [Test Connection]  [Connect]        │
└──────────────────────────────────────┘
```

**Types**:
```rust
pub struct RemoteLocation {
    host: String,
    port: u16,
    path: PathBuf,
    auth: AuthMethod,
}

pub enum AuthMethod {
    SshAgent,
    Password(String),
    KeyFile(PathBuf),
}
```

---

### Component 4: Options Panel

**Purpose**: Visual representation of all CLI options

**Location**: `src/gui/options.rs`

**UI Categories**:

**Performance**:
```
┌────────────────────────────────────┐
│ Performance                        │
├────────────────────────────────────┤
│ Buffer Size: [64  ] KB   [?]       │
│ Parallel Jobs: [4  ]     [?]       │
│ ☑ Use io_uring (Linux only)        │
│ ☑ Zero-copy (Buffer pool)          │
└────────────────────────────────────┘
```

**Metadata**:
```
┌────────────────────────────────────┐
│ Metadata Preservation              │
├────────────────────────────────────┤
│ ☑ Permissions (chmod)              │
│ ☑ Timestamps (atime/mtime)         │
│ ☑ Extended attributes              │
│ ☑ Ownership (chown)                │
│ ☑ Symlinks (copy as links)         │
└────────────────────────────────────┘
```

**Safety**:
```
┌────────────────────────────────────┐
│ Safety & Integrity                 │
├────────────────────────────────────┤
│ ☑ Fsync after write                │
│ ☑ Verify checksums                 │
│ ☐ Dry run (simulate only)          │
└────────────────────────────────────┘
```

**Each option has [?] tooltip** showing help text from CLI.

---

### Component 5: Progress Visualization

**Purpose**: Real-time copy progress display

**Location**: `src/gui/progress.rs`

**Features**:
- Overall progress bar (bytes copied / total)
- Current file being copied
- Transfer speed (MB/s)
- ETA calculation
- File count (files copied / total)
- Detailed stats (expandable)

**UI**:
```
┌──────────────────────────────────────────────────────────┐
│ Copying: /home/user/docs → /backup/docs                 │
├──────────────────────────────────────────────────────────┤
│                                                          │
│ Current: pictures/vacation.jpg (12.4 MB)                 │
│                                                          │
│ Overall Progress:                                        │
│ ████████████████████████░░░░░░░░  75% (1.5GB / 2.0GB)   │
│                                                          │
│ Speed: 250 MB/s  │  ETA: 2 seconds  │  Files: 1,234/1,500│
│                                                          │
│ ▼ Details                                                │
│   Files copied: 1,234                                    │
│   Hardlinks: 42                                          │
│   Symlinks: 8                                            │
│   Skipped: 0                                             │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

**State**:
```rust
pub struct ProgressState {
    total_bytes: u64,
    copied_bytes: u64,
    current_file: Option<PathBuf>,
    speed_mbps: f64,
    eta_seconds: u64,
    files_total: usize,
    files_copied: usize,
    stats: SyncStats,
}
```

---

## API Design

### Main GUI Entry Point

```rust
// src/gui/mod.rs

pub fn run_gui() -> iced::Result {
    ArsyncGui::run(Settings::default())
}

impl iced::Application for ArsyncGui {
    type Message = Message;
    type Executor = iced::executor::Default;  // Uses tokio by default
    type Flags = ();
    type Theme = iced::Theme;

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                source_path: PathBuf::new(),
                dest_path: PathBuf::new(),
                options: CopyOptions::default(),
                progress: None,
                status: AppStatus::Idle,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "arsync - High-Performance File Sync".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::StartCopy => {
                self.status = AppStatus::Running;
                // Spawn compio copy operation
                Command::perform(
                    start_copy_operation(
                        self.source_path.clone(),
                        self.dest_path.clone(),
                        self.options.clone(),
                    ),
                    |result| Message::CopyComplete(result),
                )
            }
            Message::ProgressUpdate(info) => {
                self.progress = Some(info);
                Command::none()
            }
            // ... other messages
        }
    }

    fn view(&self) -> Element<Message> {
        // Build UI tree
        column![
            source_panel(&self.source_path),
            dest_panel(&self.dest_path),
            options_panel(&self.options),
            progress_panel(&self.progress),
            action_buttons(&self.status),
        ].into()
    }
}
```

---

### Integration with compio

**Challenge**: iced uses tokio by default, we need compio

**Solution**: Use iced's runtime-agnostic features + bridge

```rust
// Option A: Run compio in thread, communicate via channels
use std::sync::mpsc;

let (tx, rx) = mpsc::channel();

std::thread::spawn(move || {
    compio::runtime::Runtime::new().unwrap().block_on(async {
        let result = copy_file(...).await;
        tx.send(Message::CopyComplete(result)).unwrap();
    });
});

// iced polls rx for messages


// Option B: Use iced's subscription system
impl Application for ArsyncGui {
    type Message = Message;

    fn subscription(&self) -> Subscription<Message> {
        // Create subscription that runs compio operations
        Subscription::from_recipe(CopySubscription {
            source: self.source_path.clone(),
            dest: self.dest_path.clone(),
        })
    }
}
```

---

## Data Structures

### CopyOptions

```rust
#[derive(Clone, Debug, Default)]
pub struct CopyOptions {
    // Performance
    pub buffer_size: usize,
    pub parallel_jobs: Option<usize>,
    pub use_zero_copy: bool,
    
    // Metadata
    pub preserve_permissions: bool,
    pub preserve_timestamps: bool,
    pub preserve_xattr: bool,
    pub preserve_ownership: bool,
    pub copy_symlinks: bool,
    
    // Safety
    pub fsync: bool,
    pub verify: bool,
    pub dry_run: bool,
    
    // Copy method
    pub method: CopyMethod,
}

impl From<CopyOptions> for crate::cli::Args {
    fn from(opts: CopyOptions) -> Self {
        // Convert GUI options to CLI args
    }
}
```

### AppStatus

```rust
#[derive(Clone, Debug)]
pub enum AppStatus {
    Idle,
    Connecting(String),  // "Connecting to remote..."
    Running,
    Paused,  // Future: pause/resume support
    Complete(Stats),
    Error(String),
}
```

### ProgressInfo

```rust
#[derive(Clone, Debug)]
pub struct ProgressInfo {
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub current_file: Option<PathBuf>,
    pub speed_mbps: f64,
    pub eta_seconds: Option<u64>,
    pub files_total: usize,
    pub files_copied: usize,
}
```

---

## Implementation Details

### File Structure

```
src/
├── gui/
│   ├── mod.rs           - Main application, Message enum
│   ├── dialogs.rs       - File/folder picker dialogs
│   ├── remote.rs        - Remote SSH connection UI
│   ├── options.rs       - Options panel widgets
│   ├── progress.rs      - Progress visualization
│   ├── styles.rs        - Custom styling/themes
│   └── bridge.rs        - compio ↔ iced integration
├── main.rs              - Entry point (CLI or GUI mode)
└── [existing files...]
```

### Main Entry Point Change

```rust
// src/main.rs

#[compio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    if args.gui {
        // Launch GUI mode
        #[cfg(feature = "gui")]
        return gui::run_gui().map_err(|e| /* convert error */);
        
        #[cfg(not(feature = "gui"))]
        return Err("GUI not compiled (enable 'gui' feature)".into());
    }
    
    // Existing CLI mode
    run_cli(args).await
}
```

---

### Dependencies

**New dependencies**:
```toml
[dependencies]
# GUI framework
iced = { version = "0.13", features = ["tokio", "canvas", "image"] }

# File dialogs (native)
rfd = "0.15"

# Progress tracking
indicatif = "0.17"  # Can reuse for both CLI and GUI

[features]
gui = ["iced", "rfd"]
default = []  # GUI optional, CLI always available
```

**Why iced**:
- Cross-platform with native feel
- Runtime agnostic (works with compio via threads/channels)
- Active development
- Pure Rust (no C++ toolchain)

---

### Complexity Assessment

**Overall Complexity**: Medium-High

**Breakdown**:
- **Scope**: New module (~2000 lines), main.rs changes, Cargo.toml
- **Dependencies**: 2 major new deps (iced, rfd)
- **Testing**: GUI testing complex (manual + maybe headless)
- **Risk**: Medium (GUI is separate from core logic)

**Estimated Implementation**: 3-4 phases
1. Basic UI with file selection (2-3 days)
2. Options panel integration (1-2 days)
3. Progress visualization + compio bridge (2-3 days)
4. Polish, error handling, testing (2-3 days)

**Total**: ~1-2 weeks for functional GUI

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_options_to_cli_args() {
    let opts = CopyOptions {
        preserve_permissions: true,
        buffer_size: 128 * 1024,
        ..Default::default()
    };
    
    let args: Args = opts.into();
    assert_eq!(args.buffer_size, Some(128 * 1024));
}
```

### Integration Tests

**Manual testing required** (GUI):
- File selection works
- All options correctly passed to backend
- Progress updates in real-time
- Error handling displays correctly
- Cancel operation works

### Automated Testing

**Headless testing** (if possible):
```rust
// Use iced's test utilities
#[test]
fn test_message_handling() {
    let mut app = ArsyncGui::new(());
    let cmd = app.update(Message::StartCopy);
    // Verify state changes
}
```

**Backend testing**:
- All existing tests still pass
- Progress callback integration
- CLI/GUI option parity

---

## Performance Considerations

### Expected Impact

- **CPU**: Minimal GUI overhead (~1-5% during copy)
- **Memory**: +10-20MB for GUI framework
- **I/O**: Zero impact (GUI doesn't touch I/O path)
- **Latency**: Progress updates ~100ms intervals

### Optimizations

1. **Update throttling**: Don't update UI every file (batch updates)
2. **Lazy rendering**: Only redraw when state changes
3. **Offload to compio**: All I/O in compio thread(s)
4. **Efficient state**: Use Arc for shared state if needed

### GUI Framework Overhead

**iced**:
- Binary size: +2-3MB (release build)
- Startup time: ~100-200ms
- Runtime overhead: Negligible (event-driven)

---

## Security Considerations

### Threat Model

- **Path injection**: User-selected paths (safer than typed)
- **SSH credentials**: Password/key storage
- **Remote execution**: SSH commands

### Mitigations

- ✅ File dialogs prevent most path injection
- ✅ Don't store passwords (use SSH agent when possible)
- ✅ Validate all paths before use
- ✅ Sanitize remote paths
- ✅ Use existing SSH security from arsync core

### No New Attack Surface

GUI doesn't add new security risks:
- File operations use existing safe arsync code
- SSH uses existing transport layer
- No network exposure (desktop app)

---

## Cross-Platform Considerations

### Platform-Specific Features

**Windows**:
- Native file dialogs (via rfd)
- Windows path handling (`\\?\` long paths)
- UAC for privileged operations (if needed)

**macOS**:
- Native file dialogs
- macOS-style progress indicators
- Sandbox compatibility (if sandboxed)

**Linux**:
- GTK/Qt dialogs (via rfd)
- FreeDesktop.org conventions
- io_uring available (performance advantage)

**iced handles all platform differences** - single codebase!

---

## Error Handling

### GUI-Specific Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum GuiError {
    #[error("Dialog cancelled by user")]
    DialogCancelled,
    
    #[error("Invalid path selected: {0}")]
    InvalidPath(String),
    
    #[error("Remote connection failed: {0}")]
    RemoteConnectionFailed(String),
    
    #[error("GUI framework error: {0}")]
    Framework(String),
}
```

### User-Friendly Messages

**Technical error**: `io_uring submission failed: EAGAIN`  
**GUI displays**: "System busy, retrying operation..."

**Technical error**: `Permission denied (EACCES)`  
**GUI displays**: "Cannot access destination folder. Check permissions."

---

## Migration & Compatibility

### No Breaking Changes

- GUI is **optional feature** (`--features gui`)
- CLI remains default and unchanged
- Existing scripts/automation unaffected
- Binary size increase only if GUI enabled

### Configuration

**New config** (optional):
```toml
# ~/.config/arsync/gui.toml
[gui]
default_buffer_size = 65536
remember_last_paths = true
theme = "dark"  # or "light" or "system"
```

---

## Open Questions

### Critical Questions

- [ ] **compio ↔ iced integration**: Best pattern for bridging runtimes?
  - Thread + channels?
  - Custom iced executor?
  - Hybrid approach?

- [ ] **Progress updates**: How frequent without impacting performance?
  - Every file?
  - Every N bytes?
  - Time-based (every 100ms)?

- [ ] **Remote file browser**: How to browse remote folders via SSH?
  - Execute `ls` and parse?
  - SFTP protocol?
  - Just text entry?

### Design Questions

- [ ] **Theming**: Dark/light mode support?
- [ ] **Window size**: Fixed or resizable?
- [ ] **Multiple operations**: Queue support or one-at-a-time?
- [ ] **Logging**: Show logs in GUI or separate window?

---

## Alternatives Considered

### Alternative 1: Pure CLI with TUI (terminal UI)

**Approach**: Use `ratatui` for terminal-based UI

**Pros**:
- Lighter weight than full GUI
- Works over SSH
- Still in terminal (familiar for sysadmins)

**Cons**:
- Not graphical (doesn't meet "GUI" requirement)
- Less discoverable for non-technical users
- No native file pickers

**Why not chosen**: User wants actual GUI, not TUI

---

### Alternative 2: Web-Based UI (Tauri)

**Approach**: HTML/CSS/JS frontend, Rust backend

**Pros**:
- Rich web ecosystem
- Easy to make beautiful
- Cross-platform

**Cons**:
- Large bundle size
- Web tech dependency
- Not "native" feel
- Overkill for file sync

**Why not chosen**: Prefer native, lightweight solution

---

### Alternative 3: Multiple Native GUIs

**Approach**: SwiftUI (macOS), WinUI (Windows), GTK (Linux)

**Pros**:
- Truly native on each platform
- Best platform integration

**Cons**:
- 3x implementation effort
- Hard to maintain parity
- Different languages/tools
- Not feasible for small team

**Why not chosen**: Too much effort, single codebase preferred

---

## Rollout Plan

### Phase 1: Basic GUI (MVP)

**Goals**:
- File/folder selection (local only)
- Basic options (buffer size, preserve metadata)
- Start/Cancel buttons
- Simple progress bar

**Deliverables**:
- `src/gui/mod.rs` - Basic application
- File dialog integration
- Wire up to existing copy_file()

**Time**: 3-4 days

---

### Phase 2: Full Options Support

**Goals**:
- All CLI options in GUI
- Tooltips/help text
- Option validation
- Save/load presets

**Deliverables**:
- Complete options panel
- Help system
- CLI parity

**Time**: 2-3 days

---

### Phase 3: Remote Support

**Goals**:
- SSH host/path configuration
- Connection testing
- Remote file browsing (if feasible)

**Deliverables**:
- Remote panel UI
- SSH integration
- Error handling

**Time**: 3-4 days

---

### Phase 4: Polish & Release

**Goals**:
- Detailed progress visualization
- Error recovery
- Testing on all platforms
- Documentation

**Deliverables**:
- Polished UI
- Cross-platform testing
- User guide

**Time**: 2-3 days

**Total**: ~2-3 weeks to production-ready GUI

---

## Acceptance Criteria

- [ ] GUI launches successfully on Windows, macOS, Linux
- [ ] File/folder selection works (native dialogs)
- [ ] All CLI options available in GUI
- [ ] Help text/tooltips for all options
- [ ] Real-time progress updates
- [ ] Copy operations work correctly from GUI
- [ ] Performance: <5% overhead vs CLI
- [ ] Cancel operation works
- [ ] Errors displayed user-friendly
- [ ] Remote (SSH) support functional
- [ ] Binary size reasonable (<10MB increase)
- [ ] No clippy warnings
- [ ] Builds with `--features gui`
- [ ] CLI still works without GUI feature

---

## Future Enhancements

### Phase 5+: Advanced Features

- **Queued operations**: Multiple copy jobs
- **Profiles/presets**: Save common configurations
- **History**: Recent source/dest pairs
- **Scheduling**: Delayed or recurring copies
- **Notifications**: Desktop notifications on completion
- **Drag-and-drop**: Drag folders into GUI
- **Dark/light theme**: User preference
- **Bandwidth limiting**: GUI control for network copies
- **Conflict resolution**: UI for handling conflicts

---

## References

- **iced framework**: https://github.com/iced-rs/iced
- **rfd (file dialogs)**: https://github.com/PolyMeilex/rfd
- **compio**: https://github.com/compio-rs/compio
- **Elm architecture**: https://guide.elm-lang.org/architecture/
- **Our safety docs**: `docs/safety/README.md` - Verify GUI doesn't affect safety

---

## Next Steps

1. ✅ Review this design
2. ⚠️ Decide: Proceed with iced or evaluate egui/slint more?
3. ⚠️ Answer open questions (especially compio ↔ iced integration)
4. Create implementation plan: `/plan`
5. Prototype basic file selection + copy (Phase 1)
6. Iterate based on feedback

---

**Status**: Draft - Needs review and open questions answered  
**Recommendation**: iced is the best choice for cross-platform native GUI  
**Next**: Answer integration questions, then start Phase 1 prototype

