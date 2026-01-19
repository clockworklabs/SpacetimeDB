//! Diff preview panel UI component.

use crate::subcommands::code::state::{AppState, PendingChangeStatus};
use crate::subcommands::code::ui::colors::{brand, ui as ui_colors};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Render the diff preview overlay.
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let pending: Vec<_> = state
        .pending_changes
        .iter()
        .enumerate()
        .filter(|(_, c)| c.status == PendingChangeStatus::Pending)
        .collect();

    if pending.is_empty() {
        return;
    }

    // Calculate popup area (80% of screen, centered)
    let popup_width = (area.width * 80 / 100).max(60).min(area.width.saturating_sub(4));
    let popup_height = (area.height * 80 / 100).max(20).min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Get the currently selected pending change
    let current_idx = state.selected_change_index.min(pending.len().saturating_sub(1));
    let (_, current_change) = &pending[current_idx];

    // Get the relative path for display
    let display_path = current_change
        .path
        .strip_prefix(&state.project_dir)
        .unwrap_or(&current_change.path)
        .display()
        .to_string();

    let title = format!(
        " Proposed Change: {} ({}/{}) ",
        display_path,
        current_idx + 1,
        pending.len()
    );

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(brand::PURPLE).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(brand::PURPLE))
        .style(Style::default().bg(ui_colors::BG));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Build diff content with syntax highlighting
    let mut lines: Vec<Line> = Vec::new();

    for diff_line in current_change.diff.lines() {
        let (style, prefix) = if diff_line.starts_with("+++") || diff_line.starts_with("---") {
            (Style::default().fg(ui_colors::TEXT_DIMMED), "")
        } else if diff_line.starts_with("@@") {
            (Style::default().fg(ui_colors::DIFF_HEADER), "")
        } else if diff_line.starts_with('+') {
            (Style::default().fg(ui_colors::DIFF_ADD), "")
        } else if diff_line.starts_with('-') {
            (Style::default().fg(ui_colors::DIFF_REMOVE), "")
        } else {
            (Style::default().fg(ui_colors::DIFF_CONTEXT), "")
        };

        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, diff_line),
            style,
        )));
    }

    // Add separator and instructions at the bottom
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[a]", Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)),
        Span::styled(" Accept  ", Style::default().fg(ui_colors::TEXT_MUTED)),
        Span::styled("[r]", Style::default().fg(brand::RED).add_modifier(Modifier::BOLD)),
        Span::styled(" Reject  ", Style::default().fg(ui_colors::TEXT_MUTED)),
        Span::styled("[n]", Style::default().fg(brand::YELLOW)),
        Span::styled("/", Style::default().fg(ui_colors::TEXT_DIMMED)),
        Span::styled("[p]", Style::default().fg(brand::YELLOW)),
        Span::styled(" Next/Prev  ", Style::default().fg(ui_colors::TEXT_MUTED)),
        Span::styled("[A]", Style::default().fg(brand::GREEN)),
        Span::styled(" Accept All  ", Style::default().fg(ui_colors::TEXT_MUTED)),
        Span::styled("[R]", Style::default().fg(brand::RED)),
        Span::styled(" Reject All", Style::default().fg(ui_colors::TEXT_MUTED)),
    ]));

    let diff_widget = Paragraph::new(lines).wrap(Wrap { trim: false });

    frame.render_widget(diff_widget, inner);
}

/// Render a mini diff indicator in a smaller area (for inline display).
pub fn render_mini(frame: &mut Frame, area: Rect, state: &AppState) {
    let pending_count = state
        .pending_changes
        .iter()
        .filter(|c| c.status == PendingChangeStatus::Pending)
        .count();

    if pending_count == 0 {
        return;
    }

    let indicator = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} pending change{} ", pending_count, if pending_count == 1 { "" } else { "s" }),
            Style::default().fg(ui_colors::BG).bg(brand::YELLOW),
        ),
        Span::styled(
            " Press [a] to review ",
            Style::default().fg(brand::YELLOW),
        ),
    ]));

    frame.render_widget(indicator, area);
}
