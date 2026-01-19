//! Application state for the spacetime code TUI.

use crate::util::ModuleLanguage;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

/// Maximum number of log entries to keep in memory.
pub const MAX_LOG_ENTRIES: usize = 1000;

/// Maximum number of events to keep in memory.
pub const MAX_EVENTS: usize = 100;

/// Maximum number of chat messages to keep in memory.
pub const MAX_CHAT_MESSAGES: usize = 500;

/// Available slash commands.
pub const COMMANDS: &[Command] = &[
    Command { name: "help", aliases: &["h", "?"], description: "Show help overlay" },
    Command { name: "sidebar", aliases: &["sb"], description: "Toggle sidebar panel" },
    Command { name: "clear", aliases: &["c"], description: "Clear chat history" },
    Command { name: "rebuild", aliases: &["r"], description: "Force module rebuild" },
    Command { name: "quit", aliases: &["q", "exit"], description: "Quit the application" },
];

/// A slash command definition.
pub struct Command {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

impl Command {
    /// Check if the command matches a query (prefix match on name or aliases).
    pub fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        self.name.starts_with(&query) || self.aliases.iter().any(|a| a.starts_with(&query))
    }
}

/// The main application state.
#[derive(Debug)]
pub struct AppState {
    // Project info
    pub project_dir: PathBuf,
    pub spacetimedb_dir: PathBuf,
    pub module_bindings_dir: PathBuf,
    pub database_name: String,
    pub server: String,
    pub module_language: ModuleLanguage,

    // Chat state
    pub messages: Vec<ChatMessage>,
    pub input_buffer: String,
    pub is_ai_responding: bool,
    pub current_response: String,

    // Module logs
    pub logs: VecDeque<LogEntry>,

    // File watching
    pub changed_files: Vec<ChangedFile>,
    pub regenerated_files: Vec<PathBuf>,

    // Build/publish events
    pub events: VecDeque<DevEvent>,
    pub current_build_status: BuildStatus,

    // UI state
    pub focused_panel: Panel,
    pub scroll_positions: HashMap<Panel, usize>,
    /// When true, panel auto-scrolls to bottom on new content
    pub follow_mode: HashMap<Panel, bool>,
    pub show_help: bool,
    pub show_diff_preview: bool,
    pub sidebar_collapsed: bool,
    pub command_autocomplete_index: usize,

    // Pending file changes from AI
    pub pending_changes: Vec<PendingChange>,
    pub selected_change_index: usize,
}

impl AppState {
    pub fn new(
        project_dir: PathBuf,
        spacetimedb_dir: PathBuf,
        module_bindings_dir: PathBuf,
        database_name: String,
        server: String,
        module_language: ModuleLanguage,
    ) -> Self {
        Self {
            project_dir,
            spacetimedb_dir,
            module_bindings_dir,
            database_name,
            server,
            module_language,
            messages: Vec::new(),
            input_buffer: String::new(),
            is_ai_responding: false,
            current_response: String::new(),
            logs: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            changed_files: Vec::new(),
            regenerated_files: Vec::new(),
            events: VecDeque::with_capacity(MAX_EVENTS),
            current_build_status: BuildStatus::Idle,
            focused_panel: Panel::Chat,
            scroll_positions: HashMap::new(),
            follow_mode: HashMap::new(), // Default to true (following) for all panels
            show_help: false,
            show_diff_preview: false,
            sidebar_collapsed: false,
            command_autocomplete_index: 0,
            pending_changes: Vec::new(),
            selected_change_index: 0,
        }
    }

    /// Add a chat message.
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        if self.messages.len() > MAX_CHAT_MESSAGES {
            self.messages.remove(0);
        }
    }

    /// Add a log entry.
    pub fn add_log(&mut self, log: LogEntry) {
        self.logs.push_back(log);
        while self.logs.len() > MAX_LOG_ENTRIES {
            self.logs.pop_front();
        }
    }

    /// Add a dev event.
    pub fn add_event(&mut self, event: DevEvent) {
        self.events.push_back(event);
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    /// Get the current scroll position for a panel.
    pub fn get_scroll(&self, panel: Panel) -> usize {
        *self.scroll_positions.get(&panel).unwrap_or(&0)
    }

    /// Set the scroll position for a panel.
    pub fn set_scroll(&mut self, panel: Panel, pos: usize) {
        self.scroll_positions.insert(panel, pos);
    }

    /// Check if a panel is in follow mode (auto-scroll to bottom).
    pub fn is_following(&self, panel: Panel) -> bool {
        *self.follow_mode.get(&panel).unwrap_or(&true)
    }

    /// Set follow mode for a panel.
    pub fn set_following(&mut self, panel: Panel, following: bool) {
        self.follow_mode.insert(panel, following);
    }

    /// Scroll up in a panel (disables follow mode).
    pub fn scroll_up(&mut self, panel: Panel, amount: usize) {
        let current = self.get_scroll(panel);
        let new_pos = current.saturating_sub(amount);
        self.set_scroll(panel, new_pos);
        // Scrolling up disables follow mode
        if amount > 0 && current > 0 {
            self.set_following(panel, false);
        }
    }

    /// Scroll down in a panel.
    pub fn scroll_down(&mut self, panel: Panel, amount: usize) {
        let current = self.get_scroll(panel);
        self.set_scroll(panel, current.saturating_add(amount));
        // Note: We'll check if we hit bottom in the render and re-enable follow
    }

    /// Scroll to bottom and enable follow mode.
    pub fn scroll_to_bottom(&mut self, panel: Panel) {
        self.set_scroll(panel, usize::MAX);
        self.set_following(panel, true);
    }

    /// Scroll to top (disables follow mode).
    pub fn scroll_to_top(&mut self, panel: Panel) {
        self.set_scroll(panel, 0);
        self.set_following(panel, false);
    }

    /// Cycle to the next panel.
    pub fn next_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            Panel::Chat => Panel::Logs,
            Panel::Logs => Panel::Files,
            Panel::Files => Panel::Events,
            Panel::Events => Panel::Chat,
        };
    }

    /// Cycle to the previous panel.
    pub fn prev_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            Panel::Chat => Panel::Events,
            Panel::Logs => Panel::Chat,
            Panel::Files => Panel::Logs,
            Panel::Events => Panel::Files,
        };
    }

    /// Toggle the sidebar visibility.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    /// Get matching commands for autocomplete.
    pub fn get_matching_commands(&self) -> Vec<&'static Command> {
        // Don't show suggestions for // (code comments) or non-slash input
        if !self.input_buffer.starts_with('/') || self.input_buffer.starts_with("//") {
            return Vec::new();
        }

        let query = self.input_buffer.trim_start_matches('/');
        if query.is_empty() {
            // Show all commands when just "/" is typed
            return COMMANDS.iter().collect();
        }

        // Only return commands that match
        let matches: Vec<_> = COMMANDS.iter().filter(|cmd| cmd.matches(query)).collect();

        // If no matches, return empty (don't show suggestions for unknown commands)
        matches
    }

    /// Complete the current command with the selected suggestion.
    pub fn complete_command(&mut self) {
        let matches = self.get_matching_commands();
        if !matches.is_empty() {
            let idx = self.command_autocomplete_index % matches.len();
            self.input_buffer = format!("/{}", matches[idx].name);
            self.command_autocomplete_index = 0;
        }
    }

    /// Cycle to the next command suggestion.
    pub fn next_command_suggestion(&mut self) {
        let matches = self.get_matching_commands();
        if !matches.is_empty() {
            self.command_autocomplete_index = (self.command_autocomplete_index + 1) % matches.len();
        }
    }

    /// Cycle to the previous command suggestion.
    pub fn prev_command_suggestion(&mut self) {
        let matches = self.get_matching_commands();
        if !matches.is_empty() {
            self.command_autocomplete_index = self
                .command_autocomplete_index
                .checked_sub(1)
                .unwrap_or(matches.len() - 1);
        }
    }

    /// Reset command autocomplete index (call when input changes).
    pub fn reset_command_autocomplete(&mut self) {
        self.command_autocomplete_index = 0;
    }

    /// Append text to the current AI response being streamed.
    pub fn append_to_response(&mut self, text: &str) {
        self.current_response.push_str(text);
    }

    /// Finalize the current AI response.
    pub fn finalize_response(&mut self) {
        if !self.current_response.is_empty() {
            let content = std::mem::take(&mut self.current_response);
            self.add_message(ChatMessage {
                role: MessageRole::Assistant,
                content,
            });
        }
        self.is_ai_responding = false;
    }
}

/// The focused panel in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Chat,
    Logs,
    Files,
    Events,
}

impl std::fmt::Display for Panel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Panel::Chat => write!(f, "Chat"),
            Panel::Logs => write!(f, "Logs"),
            Panel::Files => write!(f, "Files"),
            Panel::Events => write!(f, "Events"),
        }
    }
}

/// The current build status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildStatus {
    Idle,
    Building,
    Publishing,
    GeneratingBindings,
    Success,
    Error(String),
}

impl std::fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildStatus::Idle => write!(f, "Idle"),
            BuildStatus::Building => write!(f, "Building..."),
            BuildStatus::Publishing => write!(f, "Publishing..."),
            BuildStatus::GeneratingBindings => write!(f, "Generating bindings..."),
            BuildStatus::Success => write!(f, "Success"),
            BuildStatus::Error(e) => write!(f, "Error: {}", e),
        }
    }
}

/// A chat message.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

/// The role of a chat message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::User => write!(f, "You"),
            MessageRole::Assistant => write!(f, "Assistant"),
            MessageRole::System => write!(f, "System"),
        }
    }
}

/// A log entry from the module.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub level: LogLevel,
    pub message: String,
    pub target: Option<String>,
    pub filename: Option<String>,
    pub line_number: Option<u32>,
    pub function: Option<String>,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Panic,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Panic => write!(f, "PANIC"),
        }
    }
}

/// A changed file detected by the file watcher.
#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub change_type: FileChangeType,
}

/// The type of file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
}

impl std::fmt::Display for FileChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileChangeType::Created => write!(f, "created"),
            FileChangeType::Modified => write!(f, "modified"),
            FileChangeType::Deleted => write!(f, "deleted"),
        }
    }
}

/// A development event (build, publish, etc.).
#[derive(Debug, Clone)]
pub struct DevEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: DevEventType,
}

/// The type of development event.
#[derive(Debug, Clone)]
pub enum DevEventType {
    FileChanged(PathBuf),
    BuildStarted,
    BuildCompleted,
    BuildFailed(String),
    PublishStarted,
    PublishCompleted,
    PublishFailed(String),
    BindingsRegenerated(Vec<PathBuf>),
}

impl std::fmt::Display for DevEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DevEventType::FileChanged(path) => {
                write!(f, "File changed: {}", path.display())
            }
            DevEventType::BuildStarted => write!(f, "Build started"),
            DevEventType::BuildCompleted => write!(f, "Build completed"),
            DevEventType::BuildFailed(e) => write!(f, "Build failed: {}", e),
            DevEventType::PublishStarted => write!(f, "Publishing..."),
            DevEventType::PublishCompleted => write!(f, "Published successfully"),
            DevEventType::PublishFailed(e) => write!(f, "Publish failed: {}", e),
            DevEventType::BindingsRegenerated(paths) => {
                write!(f, "Bindings regenerated ({} files)", paths.len())
            }
        }
    }
}

/// A pending file change proposed by the AI.
#[derive(Debug, Clone)]
pub struct PendingChange {
    pub id: String,
    pub path: PathBuf,
    pub change_type: PendingChangeType,
    pub diff: String,
    pub status: PendingChangeStatus,
}

/// The type of pending change.
#[derive(Debug, Clone)]
pub enum PendingChangeType {
    Create { content: String },
    Edit { old: String, new: String },
    Delete,
}

/// The status of a pending change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingChangeStatus {
    Pending,
    Applied,
    Rejected,
}
