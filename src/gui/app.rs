//! Main application component for arsync GUI

use super::messages::Message;
use winio::prelude::*;

/// Main application component
///
/// This is the root component that manages the entire GUI application.
pub struct ArsyncApp {
    /// Main window
    window: Child<Window>,

    /// Application title label (temporary - will be replaced with panels)
    title_label: Child<Label>,
}

impl Component for ArsyncApp {
    type Event = (); // Root component outputs () to stop the application
    type Init<'a> = ();
    type Message = Message;

    fn init(_init: Self::Init<'_>, _sender: &ComponentSender<Self>) -> Self {
        // Create and initialize the window
        init! {
            window: Window = (()) => {
                text: "arsync - High-Performance File Sync",
                size: Size::new(900.0, 600.0),
            },
            title_label: Label = (&window) => {
                text: "arsync GUI (Phase 2: Basic Window)\n\nPress Alt+F4 or close button to exit",
                halign: HAlign::Center,
                valign: VAlign::Center,
            }
        }

        // Center window on screen
        let monitors = Monitor::all();
        if !monitors.is_empty() {
            let region = monitors[0].client_scaled();
            let window_size = window.size();
            let center = region.origin + region.size / 2.0 - window_size / 2.0;
            window.set_loc(center);
        }

        window.show();

        Self {
            window,
            title_label,
        }
    }

    async fn start(&mut self, sender: &ComponentSender<Self>) -> ! {
        // Event loop - listen to window events
        start! {
            sender, default: Message::Noop,
            self.window => {
                WindowEvent::Close => Message::WindowClose,
            }
        }
    }

    async fn update_children(&mut self) -> bool {
        // Update all child components
        futures::join!(self.window.update(), self.title_label.update(),)
            .into_iter()
            .any(|b| b)
    }

    async fn update(&mut self, message: Self::Message, sender: &ComponentSender<Self>) -> bool {
        // Handle messages
        match message {
            Message::Noop => false, // No re-render needed

            Message::WindowClose => {
                // Show confirmation dialog before closing
                match MessageBox::new()
                    .title("Close arsync?")
                    .message("Are you sure you want to exit?")
                    .instruction("Any running operations will be cancelled.")
                    .style(MessageBoxStyle::Question)
                    .buttons(MessageBoxButton::Yes | MessageBoxButton::No)
                    .show(&self.window)
                    .await
                {
                    MessageBoxResponse::Yes => {
                        // Send output event to stop the application
                        sender.output(());
                        false // No re-render needed (app is stopping)
                    }
                    _ => false, // User cancelled, stay open
                }
            }

            // Placeholder handlers for future messages
            Message::BrowseSource
            | Message::SourceSelected(_)
            | Message::BrowseDest
            | Message::DestSelected(_)
            | Message::StartCopy
            | Message::CancelCopy
            | Message::CopyComplete(_) => {
                tracing::warn!("Message not yet implemented: {:?}", message);
                false
            }
        }
    }

    fn render(&mut self, _sender: &ComponentSender<Self>) {
        // Layout the UI
        let csize = self.window.client_size();

        // Simple centered layout for now (Phase 2)
        {
            let mut panel = layout! {
                Grid::from_str("1*", "1*").unwrap(),
                self.title_label => {
                    margin: Margin::new_all_same(20.0),
                    halign: HAlign::Stretch,
                    valign: VAlign::Stretch,
                }
            };
            panel.set_size(csize);
        }
    }

    fn render_children(&mut self) {
        // Render child components
        self.title_label.render();
    }
}
