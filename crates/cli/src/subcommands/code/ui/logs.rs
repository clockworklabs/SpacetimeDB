//! Logs panel UI component.

use crate::subcommands::code::state::{AppState, LogLevel, Panel};
use crate::subcommands::code::ui::colors::{brand, ui as ui_colors};
use crate::subcommands::code::ui::panel_border_style;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Render the logs panel.
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = state.focused_panel == Panel::Logs;
    let border_style = panel_border_style(is_focused);

    let title_style = if is_focused {
        Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ui_colors::TEXT_DIMMED)
    };

    let title = format!(" Module Logs ({}) ", state.logs.len());
    let block = Block::default()
        .title(Span::styled(title, title_style))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(ui_colors::BG_HEADER));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.logs.is_empty() {
        let empty = Paragraph::new("Waiting for logs...")
            .style(Style::default().fg(ui_colors::TEXT_DIMMED));
        frame.render_widget(empty, inner);
        return;
    }

    // Calculate visible range
    let visible_lines = inner.height as usize;
    let total_logs = state.logs.len();
    let max_scroll = total_logs.saturating_sub(visible_lines);

    // If following, always show bottom; otherwise use stored scroll position
    let effective_scroll = if state.is_following(Panel::Logs) || total_logs <= visible_lines {
        max_scroll
    } else {
        state.get_scroll(Panel::Logs).min(max_scroll)
    };

    let logs: Vec<Line> = state
        .logs
        .iter()
        .skip(effective_scroll)
        .take(visible_lines)
        .map(|log| {
            let time = log
                .timestamp
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "        ".to_string());

            let (level_str, level_style) = match log.level {
                LogLevel::Error => (
                    "ERROR",
                    Style::default().fg(ui_colors::LOG_ERROR).add_modifier(Modifier::BOLD),
                ),
                LogLevel::Warn => ("WARN ", Style::default().fg(ui_colors::LOG_WARN)),
                LogLevel::Info => ("INFO ", Style::default().fg(ui_colors::LOG_INFO)),
                LogLevel::Debug => (
                    "DEBUG",
                    Style::default().fg(ui_colors::LOG_DEBUG),
                ),
                LogLevel::Trace => ("TRACE", Style::default().fg(ui_colors::LOG_TRACE)),
                LogLevel::Panic => (
                    "PANIC",
                    Style::default()
                        .fg(ui_colors::LOG_PANIC)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                ),
            };

            // Truncate message if too long
            let max_msg_len = inner.width.saturating_sub(18) as usize;
            let message = if log.message.len() > max_msg_len {
                format!("{}...", &log.message[..max_msg_len.saturating_sub(3)])
            } else {
                log.message.clone()
            };

            Line::from(vec![
                Span::styled(time, Style::default().fg(ui_colors::TEXT_DIMMED)),
                Span::raw(" "),
                Span::styled(level_str, level_style),
                Span::raw(" "),
                Span::styled(message, Style::default().fg(ui_colors::TEXT_MUTED)),
            ])
        })
        .collect();

    let logs_widget = Paragraph::new(logs);
    frame.render_widget(logs_widget, inner);
}
