//! UI rendering for the spacetime code TUI.

pub mod chat;
pub mod colors;
pub mod diff;
pub mod files;
pub mod logs;

use crate::subcommands::code::events::HELP_TEXT;
use crate::subcommands::code::state::{AppState, BuildStatus, Panel};
use colors::{brand, ui as ui_colors};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Render the entire UI.
pub fn render(frame: &mut Frame, state: &AppState) {
    let size = frame.area();

    // Fill background
    let bg = Block::default().style(Style::default().bg(ui_colors::BG));
    frame.render_widget(bg, size);

    // Main layout: header, logs, content, footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),    // Header
            Constraint::Percentage(20), // Logs (top, full width)
            Constraint::Min(10),      // Content (chat + sidebar)
            Constraint::Length(3),    // Input area
        ])
        .split(size);

    // Render header
    render_header(frame, main_chunks[0], state);

    // Render logs at top (full width, with darker background)
    let logs_bg = Block::default().style(Style::default().bg(ui_colors::BG_HEADER));
    frame.render_widget(logs_bg, main_chunks[1]);
    logs::render(frame, main_chunks[1], state);

    // Content layout: left (chat) and right (sidebar with files, events)
    let content_chunks = if state.sidebar_collapsed {
        // When collapsed, show just a thin indicator
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(10), Constraint::Length(3)])
            .split(main_chunks[2])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(main_chunks[2])
    };

    // Left side: Chat panel
    chat::render(frame, content_chunks[0], state);

    // Right side: Sidebar with solid background (now just files and events)
    if state.sidebar_collapsed {
        render_collapsed_sidebar(frame, content_chunks[1], state);
    } else {
        render_sidebar(frame, content_chunks[1], state);
    }

    // Render input area
    render_input(frame, main_chunks[3], state);

    // Render help overlay if active
    if state.show_help {
        render_help_overlay(frame, size);
    }

    // Render diff preview if there are pending changes
    if !state.pending_changes.is_empty() {
        let pending: Vec<_> = state
            .pending_changes
            .iter()
            .filter(|c| c.status == crate::subcommands::code::state::PendingChangeStatus::Pending)
            .collect();
        if !pending.is_empty() {
            diff::render(frame, size, state);
        }
    }
}

/// Render the header bar.
fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let status_icon = match &state.current_build_status {
        BuildStatus::Idle => ("●", ui_colors::SUCCESS),
        BuildStatus::Building => ("⟳", ui_colors::WARNING),
        BuildStatus::Publishing => ("⟳", ui_colors::WARNING),
        BuildStatus::GeneratingBindings => ("⟳", ui_colors::WARNING),
        BuildStatus::Success => ("✓", ui_colors::SUCCESS),
        BuildStatus::Error(_) => ("✗", ui_colors::ERROR),
    };

    let header = Line::from(vec![
        Span::styled(" ◐ ", Style::default().fg(brand::GREEN)),
        Span::styled("spacetime", Style::default().fg(ui_colors::TEXT).add_modifier(Modifier::BOLD)),
        Span::styled("DB", Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)),
        Span::styled(" code ", Style::default().fg(ui_colors::TEXT_DIMMED)),
        Span::styled("/", Style::default().fg(brand::N5)),
        Span::styled(" ", Style::default()),
        Span::styled(&state.database_name, Style::default().fg(ui_colors::TEXT)),
        Span::styled(" (", Style::default().fg(ui_colors::TEXT_DIMMED)),
        Span::styled(&state.server, Style::default().fg(ui_colors::TEXT_DIMMED)),
        Span::styled(") ", Style::default().fg(ui_colors::TEXT_DIMMED)),
        Span::styled(status_icon.0, Style::default().fg(status_icon.1)),
        Span::styled(" ", Style::default()),
        Span::styled(
            state.current_build_status.to_string(),
            Style::default().fg(status_icon.1),
        ),
        Span::raw(" ".repeat(area.width.saturating_sub(70) as usize)),
        Span::styled("[", Style::default().fg(ui_colors::TEXT_DIMMED)),
        Span::styled("?", Style::default().fg(brand::GREEN)),
        Span::styled("] Help ", Style::default().fg(ui_colors::TEXT_DIMMED)),
    ]);

    let header_widget = Paragraph::new(header).style(Style::default().bg(ui_colors::BG_HEADER));

    frame.render_widget(header_widget, area);
}

/// Render the sidebar with files and events.
fn render_sidebar(frame: &mut Frame, area: Rect, state: &AppState) {
    // Fill sidebar with solid background
    let sidebar_bg = Block::default().style(Style::default().bg(ui_colors::BG_HEADER));
    frame.render_widget(sidebar_bg, area);

    // Split into files and events (logs are now at top)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Files
            Constraint::Percentage(50), // Events
        ])
        .split(area);

    files::render(frame, chunks[0], state);
    render_events(frame, chunks[1], state);
}

/// Render the collapsed sidebar indicator.
fn render_collapsed_sidebar(frame: &mut Frame, area: Rect, _state: &AppState) {
    let block = Block::default()
        .style(Style::default().bg(ui_colors::BG_HEADER));
    frame.render_widget(block, area);

    // Vertical text indicator
    let indicator_lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled("▶", Style::default().fg(brand::GREEN))),
        Line::from(""),
        Line::from(Span::styled("S", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(Span::styled("I", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(Span::styled("D", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(Span::styled("E", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(Span::styled("B", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(Span::styled("A", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(Span::styled("R", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(""),
        Line::from(Span::styled("[", Style::default().fg(ui_colors::TEXT_DIMMED))),
        Line::from(Span::styled("\\", Style::default().fg(brand::GREEN))),
        Line::from(Span::styled("]", Style::default().fg(ui_colors::TEXT_DIMMED))),
    ];

    let indicator = Paragraph::new(indicator_lines)
        .style(Style::default().bg(ui_colors::BG_HEADER))
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(indicator, area);
}

/// Render the events panel.
fn render_events(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = state.focused_panel == Panel::Events;
    let border_style = panel_border_style(is_focused);

    let title_style = if is_focused {
        Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ui_colors::TEXT_DIMMED)
    };

    let block = Block::default()
        .title(Span::styled(" Events ", title_style))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(ui_colors::BG_HEADER));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.events.is_empty() {
        let empty = Paragraph::new("No events yet...")
            .style(Style::default().fg(ui_colors::TEXT_DIMMED));
        frame.render_widget(empty, inner);
        return;
    }

    let events: Vec<Line> = state
        .events
        .iter()
        .rev()
        .take(inner.height as usize)
        .map(|event| {
            let time = event.timestamp.format("%H:%M:%S").to_string();
            let (icon, color) = match &event.event_type {
                crate::subcommands::code::state::DevEventType::FileChanged(_) => ("●", ui_colors::WARNING),
                crate::subcommands::code::state::DevEventType::BuildStarted => ("⟳", ui_colors::WARNING),
                crate::subcommands::code::state::DevEventType::BuildCompleted => ("✓", ui_colors::SUCCESS),
                crate::subcommands::code::state::DevEventType::BuildFailed(_) => ("✗", ui_colors::ERROR),
                crate::subcommands::code::state::DevEventType::PublishStarted => ("⟳", brand::PURPLE),
                crate::subcommands::code::state::DevEventType::PublishCompleted => ("✓", ui_colors::SUCCESS),
                crate::subcommands::code::state::DevEventType::PublishFailed(_) => ("✗", ui_colors::ERROR),
                crate::subcommands::code::state::DevEventType::BindingsRegenerated(_) => ("○", brand::BLUE),
            };

            Line::from(vec![
                Span::styled(time, Style::default().fg(ui_colors::TEXT_DIMMED)),
                Span::raw(" "),
                Span::styled(icon, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(event.event_type.to_string(), Style::default().fg(ui_colors::TEXT_MUTED)),
            ])
        })
        .collect();

    let events_widget = Paragraph::new(events);
    frame.render_widget(events_widget, inner);
}

/// Render the input area.
fn render_input(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_responding = state.is_ai_responding;

    let input_style = if is_responding {
        Style::default().fg(ui_colors::TEXT_DIMMED)
    } else {
        Style::default().fg(ui_colors::TEXT)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_colors::BORDER))
        .style(Style::default().bg(ui_colors::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let prompt = if is_responding {
        Span::styled("AI is responding...", Style::default().fg(brand::PURPLE))
    } else {
        Span::styled("> ", Style::default().fg(brand::GREEN))
    };

    let input_line = Line::from(vec![
        prompt,
        Span::styled(&state.input_buffer, input_style),
    ]);

    let input_widget = Paragraph::new(input_line)
        .wrap(Wrap { trim: false });

    frame.render_widget(input_widget, inner);

    // Show cursor if not responding
    if !is_responding {
        let cursor_x = inner.x + 2 + state.input_buffer.len() as u16;
        let cursor_y = inner.y;
        if cursor_x < inner.x + inner.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    // Show command autocomplete suggestions
    let matches = state.get_matching_commands();
    if !matches.is_empty() && state.input_buffer.starts_with('/') {
        render_command_suggestions(frame, area, &matches, state.command_autocomplete_index);
    }
}

/// Render command autocomplete suggestions above the input area.
fn render_command_suggestions(
    frame: &mut Frame,
    input_area: Rect,
    matches: &[&crate::subcommands::code::state::Command],
    selected_index: usize,
) {
    let height = (matches.len() as u16 + 2).min(10);
    let width = 40.min(input_area.width.saturating_sub(4));

    // Position above the input area
    let popup_area = Rect {
        x: input_area.x + 2,
        y: input_area.y.saturating_sub(height),
        width,
        height,
    };

    // Clear the area
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" Commands ", Style::default().fg(brand::GREEN)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_colors::BORDER))
        .style(Style::default().bg(ui_colors::BG_HEADER));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let lines: Vec<Line> = matches
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let is_selected = i == selected_index % matches.len();
            let style = if is_selected {
                Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(ui_colors::TEXT)
            };
            let desc_style = if is_selected {
                Style::default().fg(ui_colors::TEXT_MUTED)
            } else {
                Style::default().fg(ui_colors::TEXT_DIMMED)
            };

            let prefix = if is_selected { "▶ " } else { "  " };

            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("/{}", cmd.name), style),
                Span::styled(format!("  {}", cmd.description), desc_style),
            ])
        })
        .collect();

    let suggestions = Paragraph::new(lines);
    frame.render_widget(suggestions, inner);
}

/// Render the help overlay.
fn render_help_overlay(frame: &mut Frame, area: Rect) {
    // Calculate centered popup area
    let popup_width = 55.min(area.width.saturating_sub(4));
    let popup_height = 28.min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" Help ", Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(brand::GREEN))
        .style(Style::default().bg(ui_colors::BG_HEADER));

    let help = Paragraph::new(HELP_TEXT.trim())
        .block(block)
        .style(Style::default().fg(ui_colors::TEXT_MUTED))
        .wrap(Wrap { trim: false });

    frame.render_widget(help, popup_area);
}

/// Get the border style for a panel based on focus state.
pub fn panel_border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(brand::GREEN)
    } else {
        Style::default().fg(ui_colors::BORDER)
    }
}
