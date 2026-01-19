//! Event types and handling for the spacetime code TUI.

use crate::subcommands::code::state::LogEntry;
use crate::subcommands::code::tools::ToolCall;
use crossterm::event::KeyEvent;
use std::path::PathBuf;

/// Application events that drive the TUI.
#[derive(Debug)]
pub enum AppEvent {
    // User input events
    KeyPress(KeyEvent),
    /// Mouse scroll event: (delta_y, row, col) - negative delta = scroll up
    MouseScroll(i32, u16, u16),

    // File system events
    FileChanged(PathBuf),

    // Build/publish events
    BuildStarted,
    BuildCompleted,
    BuildFailed(String),
    PublishStarted,
    PublishCompleted,
    PublishFailed(String),
    BindingsRegenerated(Vec<PathBuf>),

    // Log streaming events
    LogReceived(LogEntry),
    LogStreamError(String),
    LogStreamReconnecting,

    // AI events
    AiResponseChunk(String),
    AiResponseComplete,
    AiError(String),
    AiToolCall(ToolCall),

    // System events
    Tick,
    Quit,
    Resize(u16, u16),
}

/// Actions that can be triggered by the user.
#[derive(Debug, Clone)]
pub enum UserAction {
    /// Send the current input as a chat message.
    SendMessage,

    /// Autocomplete/cycle command suggestion.
    AutocompleteCommand,

    /// Cycle to previous command suggestion.
    AutocompletePrev,

    /// Scroll up in the focused panel.
    ScrollUp,

    /// Scroll down in the focused panel.
    ScrollDown,

    /// Page up in the focused panel.
    PageUp,

    /// Page down in the focused panel.
    PageDown,

    /// Go to the top of the focused panel.
    ScrollToTop,

    /// Go to the bottom of the focused panel.
    ScrollToBottom,

    /// Accept the currently selected pending change.
    AcceptChange,

    /// Reject the currently selected pending change.
    RejectChange,

    /// Select the next pending change.
    NextChange,

    /// Select the previous pending change.
    PrevChange,

    /// Accept all pending changes.
    AcceptAllChanges,

    /// Reject all pending changes.
    RejectAllChanges,

    /// Close any open overlay/dialog.
    CloseOverlay,

    /// Quit the application.
    Quit,
}

/// Parse a key event into a user action.
pub fn parse_key_event(key: KeyEvent, has_pending_changes: bool) -> Option<UserAction> {
    use crossterm::event::{KeyCode, KeyModifiers};

    match (key.modifiers, key.code) {
        // Quit
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(UserAction::Quit),

        // Close overlay/dialog
        (KeyModifiers::NONE, KeyCode::Esc) => Some(UserAction::CloseOverlay),

        // Autocomplete
        (KeyModifiers::NONE, KeyCode::Tab) => Some(UserAction::AutocompleteCommand),
        (KeyModifiers::SHIFT, KeyCode::BackTab) => Some(UserAction::AutocompletePrev),

        // Scrolling
        (KeyModifiers::NONE, KeyCode::Up) => Some(UserAction::ScrollUp),
        (KeyModifiers::NONE, KeyCode::Down) => Some(UserAction::ScrollDown),
        (KeyModifiers::NONE, KeyCode::PageUp) => Some(UserAction::PageUp),
        (KeyModifiers::NONE, KeyCode::PageDown) => Some(UserAction::PageDown),
        (KeyModifiers::NONE, KeyCode::Home) => Some(UserAction::ScrollToTop),
        (KeyModifiers::NONE, KeyCode::End) => Some(UserAction::ScrollToBottom),

        // Pending change actions (when there are pending changes)
        (KeyModifiers::NONE, KeyCode::Char('a')) if has_pending_changes => Some(UserAction::AcceptChange),
        (KeyModifiers::NONE, KeyCode::Char('r')) if has_pending_changes => Some(UserAction::RejectChange),
        (KeyModifiers::NONE, KeyCode::Char('n')) if has_pending_changes => Some(UserAction::NextChange),
        (KeyModifiers::NONE, KeyCode::Char('p')) if has_pending_changes => Some(UserAction::PrevChange),
        (KeyModifiers::SHIFT, KeyCode::Char('A')) if has_pending_changes => Some(UserAction::AcceptAllChanges),
        (KeyModifiers::SHIFT, KeyCode::Char('R')) if has_pending_changes => Some(UserAction::RejectAllChanges),

        // Send message (Enter)
        (KeyModifiers::NONE, KeyCode::Enter) => Some(UserAction::SendMessage),

        _ => None,
    }
}

/// Help text for commands and shortcuts.
pub const HELP_TEXT: &str = r#"
Slash Commands
==============
/help       Show this help
/sidebar    Toggle sidebar
/clear      Clear chat history
/rebuild    Force rebuild
/quit       Quit the application

Type / to see all commands.
Tab to autocomplete, Shift+Tab to cycle back.

AI File Operations
------------------
The AI can read and write files in your project:
- read_file: Reads files (automatic)
- list_files: Lists directory contents (automatic)
- write_file: Creates/overwrites files (needs approval)
- edit_file: Makes targeted edits (needs approval)

When the AI proposes a file change, you'll see
a diff preview and can accept or reject it.

Navigation
----------
↑/↓         Scroll in focused panel
Mouse/Track Scroll with trackpad or mouse wheel
PgUp/PgDn   Page up/down
Home/End    Go to top/bottom

File Changes (when AI proposes edits)
-------------------------------------
a           Accept current change
r           Reject current change
n           Next change
p           Previous change
A           Accept all changes
R           Reject all changes

General
-------
Ctrl+C      Quit
Esc         Close help/dialogs
"#;
