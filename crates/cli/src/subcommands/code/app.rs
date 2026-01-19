//! Main application event loop for the spacetime code TUI.

use crate::config::Config;
use crate::subcommands::code::ai::{AiClient, AiContext, AiStreamEvent};
use crate::subcommands::code::events::{parse_key_event, AppEvent, UserAction};
use crate::subcommands::code::state::{
    AppState, BuildStatus, ChangedFile, ChatMessage, DevEvent, DevEventType, FileChangeType, LogEntry,
    LogLevel, MessageRole, Panel, PendingChangeStatus,
};
use crate::subcommands::code::tools::{FileTools, ToolResult};
use crate::subcommands::code::ui;
use crate::util::{add_auth_header_opt, get_auth_header};
use anyhow::{Context, Result};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyModifiers, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use futures::{AsyncBufReadExt, StreamExt, TryStreamExt};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::borrow::Cow;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;

/// Run the main application loop.
pub async fn run(
    config: Config,
    state: AppState,
    _auth_token: Option<String>,
) -> Result<()> {
    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Create event channels
    let (event_tx, mut event_rx) = tokio_mpsc::channel::<AppEvent>(100);

    // Create mutable state
    let mut state = state;

    // Spawn background tasks
    let _file_watcher = spawn_file_watcher(event_tx.clone(), &state.spacetimedb_dir)?;
    let _log_task = spawn_log_streamer(
        event_tx.clone(),
        config.clone(),
        state.database_name.clone(),
        state.server.clone(),
    );
    spawn_input_handler(event_tx.clone());
    spawn_tick_timer(event_tx.clone());

    // Create AI client - uses OPENAI_API_KEY env var for now
    let ai_client = {
        let client = AiClient::new(String::new(), String::new());
        if client.has_api_key() {
            Some(client)
        } else {
            None
        }
    };

    // Create file tools
    let file_tools = FileTools::new(state.project_dir.clone())?;

    // Add initial event
    state.add_event(DevEvent {
        timestamp: chrono::Utc::now(),
        event_type: DevEventType::BuildStarted,
    });

    // Main event loop
    loop {
        // Render UI
        terminal.draw(|f| ui::render(f, &state))?;

        // Handle events
        match event_rx.recv().await {
            Some(AppEvent::Quit) => break,
            Some(AppEvent::KeyPress(key)) => {
                let has_pending = state
                    .pending_changes
                    .iter()
                    .any(|c| c.status == PendingChangeStatus::Pending);

                // Parse key into action first (for special keys like ?, \, Ctrl+C, etc.)
                if let Some(action) = parse_key_event(key, has_pending) {
                    handle_user_action(
                        action,
                        &mut state,
                        &ai_client,
                        &file_tools,
                        &event_tx,
                    )
                    .await?;
                } else if state.focused_panel == Panel::Chat && !has_pending && !state.is_ai_responding {
                    // Handle text input when in chat panel and no pending changes
                    match (key.modifiers, key.code) {
                        (KeyModifiers::NONE, KeyCode::Char(c)) => {
                            state.input_buffer.push(c);
                            state.reset_command_autocomplete();
                        }
                        (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                            state.input_buffer.push(c.to_ascii_uppercase());
                            state.reset_command_autocomplete();
                        }
                        (KeyModifiers::NONE, KeyCode::Backspace) => {
                            state.input_buffer.pop();
                            state.reset_command_autocomplete();
                        }
                        _ => {}
                    }
                }
            }
            Some(AppEvent::FileChanged(path)) => {
                handle_file_changed(&mut state, path, &event_tx).await;
            }
            Some(AppEvent::BuildStarted) => {
                state.current_build_status = BuildStatus::Building;
                state.add_event(DevEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: DevEventType::BuildStarted,
                });
            }
            Some(AppEvent::BuildCompleted) => {
                state.current_build_status = BuildStatus::Success;
                state.add_event(DevEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: DevEventType::BuildCompleted,
                });
            }
            Some(AppEvent::BuildFailed(error)) => {
                state.current_build_status = BuildStatus::Error(error.clone());
                state.add_event(DevEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: DevEventType::BuildFailed(error),
                });
            }
            Some(AppEvent::PublishStarted) => {
                state.current_build_status = BuildStatus::Publishing;
                state.add_event(DevEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: DevEventType::PublishStarted,
                });
            }
            Some(AppEvent::PublishCompleted) => {
                state.current_build_status = BuildStatus::Success;
                state.add_event(DevEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: DevEventType::PublishCompleted,
                });
            }
            Some(AppEvent::PublishFailed(error)) => {
                state.current_build_status = BuildStatus::Error(error.clone());
                state.add_event(DevEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: DevEventType::PublishFailed(error),
                });
            }
            Some(AppEvent::BindingsRegenerated(paths)) => {
                state.regenerated_files = paths.clone();
                state.add_event(DevEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: DevEventType::BindingsRegenerated(paths),
                });
            }
            Some(AppEvent::LogReceived(log)) => {
                state.add_log(log);
            }
            Some(AppEvent::LogStreamError(_error)) => {
                // Log stream errors are handled silently, will reconnect
            }
            Some(AppEvent::LogStreamReconnecting) => {
                // Silently reconnecting
            }
            Some(AppEvent::AiResponseChunk(chunk)) => {
                state.append_to_response(&chunk);
            }
            Some(AppEvent::AiResponseComplete) => {
                state.finalize_response();
            }
            Some(AppEvent::AiError(error)) => {
                state.add_message(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Error: {}", error),
                });
                state.is_ai_responding = false;
                state.current_response.clear();
            }
            Some(AppEvent::AiToolCall(tool_call)) => {
                // Execute the tool and handle the result
                let result = file_tools.execute(&tool_call);
                match result {
                    ToolResult::Success(output) => {
                        // For read operations, send result back to AI
                        state.add_message(ChatMessage {
                            role: MessageRole::System,
                            content: format!("[Tool: {}]\n{}", tool_call.name, output),
                        });

                        // Continue the AI conversation with the tool result
                        if let Some(client) = &ai_client {
                            let messages = state.messages.clone();
                            let context = AiContext::new(&state.database_name, state.module_language);
                            let client = client.clone();
                            let event_tx = event_tx.clone();
                            let tool_id = tool_call.id.clone();
                            let tool_name = tool_call.name.clone();

                            tokio::spawn(async move {
                                match client.send_tool_result(messages, tool_id, tool_name, output, context).await {
                                    Ok(mut stream) => {
                                        while let Some(event_result) = stream.next().await {
                                            match event_result {
                                                Ok(AiStreamEvent::Content(content)) => {
                                                    event_tx.send(AppEvent::AiResponseChunk(content)).await.ok();
                                                }
                                                Ok(AiStreamEvent::ToolCall(tc)) => {
                                                    event_tx.send(AppEvent::AiToolCall(tc)).await.ok();
                                                }
                                                Ok(AiStreamEvent::Done(_)) => {
                                                    event_tx.send(AppEvent::AiResponseComplete).await.ok();
                                                    break;
                                                }
                                                Ok(AiStreamEvent::Error(error)) => {
                                                    event_tx.send(AppEvent::AiError(error)).await.ok();
                                                    break;
                                                }
                                                Err(e) => {
                                                    event_tx.send(AppEvent::AiError(e.to_string())).await.ok();
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        event_tx.send(AppEvent::AiError(e.to_string())).await.ok();
                                    }
                                }
                            });
                        }
                    }
                    ToolResult::NeedsApproval(mut change) => {
                        // Store the tool call info for later
                        change.id = tool_call.id.clone();
                        state.pending_changes.push(change);
                        // Pause AI response until user approves/rejects
                        state.is_ai_responding = false;
                    }
                    ToolResult::Error(error) => {
                        state.add_message(ChatMessage {
                            role: MessageRole::System,
                            content: format!("Tool '{}' error: {}", tool_call.name, error),
                        });
                    }
                }
            }
            Some(AppEvent::MouseScroll(delta, _row, _col)) => {
                // Scroll the focused panel based on mouse wheel
                let panel = state.focused_panel;
                if delta < 0 {
                    // Scroll up (away from bottom)
                    state.scroll_up(panel, (-delta) as usize);
                } else {
                    // Scroll down (toward bottom)
                    state.scroll_down(panel, delta as usize);
                }
            }
            Some(AppEvent::Tick) => {
                // Just triggers a redraw
            }
            Some(AppEvent::Resize(_, _)) => {
                // Terminal resize is handled automatically by ratatui
            }
            None => break,
        }
    }

    // Restore terminal
    restore_terminal(&mut terminal)?;
    Ok(())
}

/// Handle a user action.
async fn handle_user_action(
    action: UserAction,
    state: &mut AppState,
    ai_client: &Option<AiClient>,
    file_tools: &FileTools,
    event_tx: &tokio_mpsc::Sender<AppEvent>,
) -> Result<()> {
    match action {
        UserAction::Quit => {
            event_tx.send(AppEvent::Quit).await.ok();
        }
        UserAction::SendMessage => {
            if state.input_buffer.is_empty() || state.is_ai_responding {
                return Ok(());
            }

            // Check if command popup is showing - if so, use selected command
            let matches = state.get_matching_commands();
            let resolved_cmd = if !matches.is_empty() {
                let idx = state.command_autocomplete_index % matches.len();
                Some(matches[idx].name)
            } else {
                None
            };

            // Reset autocomplete state
            state.command_autocomplete_index = 0;

            // If we have a selected command from the popup, execute it
            if let Some(cmd) = resolved_cmd {
                state.input_buffer.clear();
                match cmd {
                    "help" => {
                        state.show_help = !state.show_help;
                    }
                    "sidebar" => {
                        state.toggle_sidebar();
                    }
                    "clear" => {
                        state.messages.clear();
                        state.current_response.clear();
                    }
                    "rebuild" => {
                        event_tx.send(AppEvent::BuildStarted).await.ok();
                    }
                    "quit" => {
                        event_tx.send(AppEvent::Quit).await.ok();
                    }
                    _ => {}
                }
                return Ok(());
            }

            let message = std::mem::take(&mut state.input_buffer);

            state.add_message(ChatMessage {
                role: MessageRole::User,
                content: message.clone(),
            });

            // Send to AI if client is available
            if let Some(client) = ai_client {
                state.is_ai_responding = true;

                // Build context
                let recent_logs: Vec<_> = state.logs.iter().rev().take(20).cloned().collect();
                let context = AiContext::new(&state.database_name, state.module_language)
                    .with_recent_logs(&recent_logs);

                // Clone what we need for the async task
                let messages: Vec<ChatMessage> = state.messages.clone();
                let client = client.clone();
                let event_tx = event_tx.clone();

                // Spawn async task to handle streaming response
                tokio::spawn(async move {
                    match client.chat_stream(messages, context).await {
                        Ok(mut stream) => {
                            while let Some(event_result) = stream.next().await {
                                match event_result {
                                    Ok(AiStreamEvent::Content(content)) => {
                                        event_tx.send(AppEvent::AiResponseChunk(content)).await.ok();
                                    }
                                    Ok(AiStreamEvent::ToolCall(tool_call)) => {
                                        event_tx.send(AppEvent::AiToolCall(tool_call)).await.ok();
                                    }
                                    Ok(AiStreamEvent::Done(_)) => {
                                        event_tx.send(AppEvent::AiResponseComplete).await.ok();
                                        break;
                                    }
                                    Ok(AiStreamEvent::Error(error)) => {
                                        event_tx.send(AppEvent::AiError(error)).await.ok();
                                        break;
                                    }
                                    Err(e) => {
                                        event_tx.send(AppEvent::AiError(e.to_string())).await.ok();
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            event_tx.send(AppEvent::AiError(e.to_string())).await.ok();
                        }
                    }
                });
            } else {
                state.add_message(ChatMessage {
                    role: MessageRole::System,
                    content: "AI assistant is not available. Set OPENAI_API_KEY environment variable to enable.".to_string(),
                });
            }
        }
        UserAction::AutocompleteCommand => {
            if state.input_buffer.starts_with('/') {
                let matches = state.get_matching_commands();
                if matches.len() == 1 {
                    // Only one match, complete it
                    state.complete_command();
                } else if !matches.is_empty() {
                    // Multiple matches, cycle through and complete
                    state.complete_command();
                    state.next_command_suggestion();
                }
            }
        }
        UserAction::AutocompletePrev => {
            if state.input_buffer.starts_with('/') {
                state.prev_command_suggestion();
                state.complete_command();
            }
        }
        UserAction::ScrollUp => {
            // If command popup is showing, navigate suggestions instead
            if !state.get_matching_commands().is_empty() {
                state.prev_command_suggestion();
                return Ok(());
            }
            let panel = state.focused_panel;
            state.scroll_up(panel, 1);
        }
        UserAction::ScrollDown => {
            // If command popup is showing, navigate suggestions instead
            if !state.get_matching_commands().is_empty() {
                state.next_command_suggestion();
                return Ok(());
            }
            let panel = state.focused_panel;
            state.scroll_down(panel, 1);
        }
        UserAction::PageUp => {
            let panel = state.focused_panel;
            state.scroll_up(panel, 10);
        }
        UserAction::PageDown => {
            let panel = state.focused_panel;
            state.scroll_down(panel, 10);
        }
        UserAction::ScrollToTop => {
            let panel = state.focused_panel;
            state.scroll_to_top(panel);
        }
        UserAction::ScrollToBottom => {
            let panel = state.focused_panel;
            state.scroll_to_bottom(panel);
        }
        UserAction::AcceptChange => {
            let pending: Vec<_> = state
                .pending_changes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.status == PendingChangeStatus::Pending)
                .map(|(i, _)| i)
                .collect();

            if let Some(&idx) = pending.get(state.selected_change_index.min(pending.len().saturating_sub(1))) {
                let tool_id = state.pending_changes[idx].id.clone();
                let path = state.pending_changes[idx].path.display().to_string();

                if let Err(e) = file_tools.apply_change(&state.pending_changes[idx]) {
                    let error_msg = format!("Failed to apply change: {}", e);
                    state.add_message(ChatMessage {
                        role: MessageRole::System,
                        content: error_msg.clone(),
                    });

                    // Send error result back to AI
                    if let Some(client) = ai_client {
                        send_tool_result_to_ai(
                            client.clone(),
                            state.messages.clone(),
                            tool_id,
                            "write_file".to_string(),
                            error_msg,
                            AiContext::new(&state.database_name, state.module_language),
                            event_tx.clone(),
                        );
                    }
                } else {
                    state.pending_changes[idx].status = PendingChangeStatus::Applied;
                    let success_msg = format!("Successfully wrote to {}", path);
                    state.add_message(ChatMessage {
                        role: MessageRole::System,
                        content: success_msg.clone(),
                    });

                    // Send success result back to AI to continue
                    if let Some(client) = ai_client {
                        state.is_ai_responding = true;
                        send_tool_result_to_ai(
                            client.clone(),
                            state.messages.clone(),
                            tool_id,
                            "write_file".to_string(),
                            success_msg,
                            AiContext::new(&state.database_name, state.module_language),
                            event_tx.clone(),
                        );
                    }
                }
            }
        }
        UserAction::RejectChange => {
            let pending: Vec<_> = state
                .pending_changes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.status == PendingChangeStatus::Pending)
                .map(|(i, _)| i)
                .collect();

            if let Some(&idx) = pending.get(state.selected_change_index.min(pending.len().saturating_sub(1))) {
                let tool_id = state.pending_changes[idx].id.clone();
                let path = state.pending_changes[idx].path.display().to_string();

                state.pending_changes[idx].status = PendingChangeStatus::Rejected;
                let reject_msg = format!("User rejected the change to {}", path);
                state.add_message(ChatMessage {
                    role: MessageRole::System,
                    content: reject_msg.clone(),
                });

                // Send rejection result back to AI
                if let Some(client) = ai_client {
                    state.is_ai_responding = true;
                    send_tool_result_to_ai(
                        client.clone(),
                        state.messages.clone(),
                        tool_id,
                        "write_file".to_string(),
                        reject_msg,
                        AiContext::new(&state.database_name, state.module_language),
                        event_tx.clone(),
                    );
                }
            }
        }
        UserAction::NextChange => {
            let pending_count = state
                .pending_changes
                .iter()
                .filter(|c| c.status == PendingChangeStatus::Pending)
                .count();
            if pending_count > 0 {
                state.selected_change_index = (state.selected_change_index + 1) % pending_count;
            }
        }
        UserAction::PrevChange => {
            let pending_count = state
                .pending_changes
                .iter()
                .filter(|c| c.status == PendingChangeStatus::Pending)
                .count();
            if pending_count > 0 {
                state.selected_change_index = state
                    .selected_change_index
                    .checked_sub(1)
                    .unwrap_or(pending_count - 1);
            }
        }
        UserAction::AcceptAllChanges => {
            let mut errors: Vec<String> = Vec::new();
            for i in 0..state.pending_changes.len() {
                if state.pending_changes[i].status == PendingChangeStatus::Pending {
                    if let Err(e) = file_tools.apply_change(&state.pending_changes[i]) {
                        errors.push(format!(
                            "Failed to apply change to {}: {}",
                            state.pending_changes[i].path.display(),
                            e
                        ));
                    } else {
                        state.pending_changes[i].status = PendingChangeStatus::Applied;
                    }
                }
            }
            for error in errors {
                state.add_message(ChatMessage {
                    role: MessageRole::System,
                    content: error,
                });
            }
            state.add_message(ChatMessage {
                role: MessageRole::System,
                content: "Applied all pending changes".to_string(),
            });
        }
        UserAction::RejectAllChanges => {
            for change in &mut state.pending_changes {
                if change.status == PendingChangeStatus::Pending {
                    change.status = PendingChangeStatus::Rejected;
                }
            }
            state.add_message(ChatMessage {
                role: MessageRole::System,
                content: "Rejected all pending changes".to_string(),
            });
        }
        UserAction::CloseOverlay => {
            // Close help if open, otherwise do nothing
            if state.show_help {
                state.show_help = false;
            }
        }
    }

    Ok(())
}

/// Handle a file change event.
async fn handle_file_changed(
    state: &mut AppState,
    path: PathBuf,
    event_tx: &tokio_mpsc::Sender<AppEvent>,
) {
    // Add to changed files list
    let change_type = if path.exists() {
        FileChangeType::Modified
    } else {
        FileChangeType::Deleted
    };

    // Check if file is already in the list
    if let Some(existing) = state.changed_files.iter_mut().find(|f| f.path == path) {
        existing.change_type = change_type;
    } else {
        state.changed_files.push(ChangedFile {
            path: path.clone(),
            change_type,
        });
    }

    // Add event
    state.add_event(DevEvent {
        timestamp: chrono::Utc::now(),
        event_type: DevEventType::FileChanged(path),
    });

    // Trigger rebuild
    event_tx.send(AppEvent::BuildStarted).await.ok();
}

/// Setup the terminal for TUI mode.
fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).context("Failed to create terminal")?;
    Ok(terminal)
}

/// Restore the terminal to normal mode.
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture).context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;
    Ok(())
}

/// Spawn the file watcher task.
fn spawn_file_watcher(
    event_tx: tokio_mpsc::Sender<AppEvent>,
    spacetimedb_dir: &PathBuf,
) -> Result<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = Watcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    notify::EventKind::Modify(_) | notify::EventKind::Create(_) | notify::EventKind::Remove(_)
                ) {
                    let _ = tx.send(event.paths);
                }
            }
        },
        notify::Config::default().with_poll_interval(Duration::from_millis(500)),
    )?;

    let src_dir = spacetimedb_dir.join("src");
    watcher.watch(&src_dir, RecursiveMode::Recursive)?;

    // Spawn a thread to forward events
    let event_tx_clone = event_tx.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Handle::current();
        loop {
            if let Ok(paths) = rx.recv() {
                for path in paths {
                    let event_tx = event_tx_clone.clone();
                    rt.spawn(async move {
                        event_tx.send(AppEvent::FileChanged(path)).await.ok();
                    });
                }
            }
        }
    });

    Ok(watcher)
}

/// Spawn the log streaming task.
fn spawn_log_streamer(
    event_tx: tokio_mpsc::Sender<AppEvent>,
    config: Config,
    database_name: String,
    server: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(e) = stream_logs(&event_tx, config.clone(), &database_name, &server).await {
                tracing::warn!("Log streaming error: {}", e);
                event_tx.send(AppEvent::LogStreamError(e.to_string())).await.ok();
                tokio::time::sleep(Duration::from_secs(5)).await;
                event_tx.send(AppEvent::LogStreamReconnecting).await.ok();
            }
        }
    })
}

/// Stream logs from the server.
async fn stream_logs(
    event_tx: &tokio_mpsc::Sender<AppEvent>,
    mut config: Config,
    database_name: &str,
    server: &str,
) -> Result<()> {
    let host_url = config.get_host_url(Some(server))?;
    let auth_header = get_auth_header(&mut config, false, Some(server), false).await?;

    // Get database identity
    let database_identity = crate::util::database_identity(&config, database_name, Some(server)).await?;

    let client = reqwest::Client::new();
    let builder = client.get(format!("{}/v1/database/{}/logs", host_url, database_identity.to_hex()));
    let builder = add_auth_header_opt(builder, &auth_header);
    let res = builder
        .query(&[("num_lines", "10"), ("follow", "true")])
        .send()
        .await?;

    let status = res.status();
    if status.is_client_error() || status.is_server_error() {
        let err = res.text().await?;
        anyhow::bail!(err)
    }

    let mut rdr = res.bytes_stream().map_err(std::io::Error::other).into_async_read();
    let mut line = String::new();

    while rdr.read_line(&mut line).await? != 0 {
        if let Ok(record) = serde_json::from_str::<LogRecord<'_>>(&line) {
            let log_entry = LogEntry {
                timestamp: record.ts,
                level: match record.level {
                    ApiLogLevel::Error => LogLevel::Error,
                    ApiLogLevel::Warn => LogLevel::Warn,
                    ApiLogLevel::Info => LogLevel::Info,
                    ApiLogLevel::Debug => LogLevel::Debug,
                    ApiLogLevel::Trace => LogLevel::Trace,
                    ApiLogLevel::Panic => LogLevel::Panic,
                },
                message: record.message.to_string(),
                target: record.target.map(|t| t.to_string()),
                filename: record.filename.map(|f| f.to_string()),
                line_number: record.line_number,
                function: record.function.map(|f| f.to_string()),
            };
            event_tx.send(AppEvent::LogReceived(log_entry)).await.ok();
        }
        line.clear();
    }

    Ok(())
}

/// Spawn the input handler task.
fn spawn_input_handler(event_tx: tokio_mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(event_result) = reader.next().await {
            match event_result {
                Ok(Event::Key(key)) => {
                    event_tx.send(AppEvent::KeyPress(key)).await.ok();
                }
                Ok(Event::Mouse(mouse)) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            event_tx.send(AppEvent::MouseScroll(-3, mouse.row, mouse.column)).await.ok();
                        }
                        MouseEventKind::ScrollDown => {
                            event_tx.send(AppEvent::MouseScroll(3, mouse.row, mouse.column)).await.ok();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Resize(w, h)) => {
                    event_tx.send(AppEvent::Resize(w, h)).await.ok();
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
}

/// Spawn the tick timer task.
fn spawn_tick_timer(event_tx: tokio_mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            interval.tick().await;
            if event_tx.send(AppEvent::Tick).await.is_err() {
                break;
            }
        }
    });
}

// Log record from the API
#[derive(serde::Deserialize)]
enum ApiLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Panic,
}

#[serde_with::serde_as]
#[derive(serde::Deserialize)]
struct LogRecord<'a> {
    #[serde_as(as = "Option<serde_with::TimestampMicroSeconds>")]
    ts: Option<chrono::DateTime<chrono::Utc>>,
    level: ApiLogLevel,
    #[serde(borrow)]
    target: Option<Cow<'a, str>>,
    #[serde(borrow)]
    filename: Option<Cow<'a, str>>,
    line_number: Option<u32>,
    #[serde(borrow)]
    function: Option<Cow<'a, str>>,
    #[serde(borrow)]
    message: Cow<'a, str>,
}

/// Helper to send a tool result back to the AI and continue the conversation.
fn send_tool_result_to_ai(
    client: AiClient,
    messages: Vec<ChatMessage>,
    tool_id: String,
    tool_name: String,
    result: String,
    context: AiContext,
    event_tx: tokio_mpsc::Sender<AppEvent>,
) {
    tokio::spawn(async move {
        match client.send_tool_result(messages, tool_id, tool_name, result, context).await {
            Ok(mut stream) => {
                while let Some(event_result) = stream.next().await {
                    match event_result {
                        Ok(AiStreamEvent::Content(content)) => {
                            event_tx.send(AppEvent::AiResponseChunk(content)).await.ok();
                        }
                        Ok(AiStreamEvent::ToolCall(tc)) => {
                            event_tx.send(AppEvent::AiToolCall(tc)).await.ok();
                        }
                        Ok(AiStreamEvent::Done(_)) => {
                            event_tx.send(AppEvent::AiResponseComplete).await.ok();
                            break;
                        }
                        Ok(AiStreamEvent::Error(error)) => {
                            event_tx.send(AppEvent::AiError(error)).await.ok();
                            break;
                        }
                        Err(e) => {
                            event_tx.send(AppEvent::AiError(e.to_string())).await.ok();
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                event_tx.send(AppEvent::AiError(e.to_string())).await.ok();
            }
        }
    });
}
