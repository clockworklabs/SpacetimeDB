//! Files panel UI component.

use crate::subcommands::code::state::{AppState, FileChangeType, Panel};
use crate::subcommands::code::ui::colors::{brand, ui as ui_colors};
use crate::subcommands::code::ui::panel_border_style;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Render the files panel.
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = state.focused_panel == Panel::Files;
    let border_style = panel_border_style(is_focused);

    let title_style = if is_focused {
        Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ui_colors::TEXT_DIMMED)
    };

    let block = Block::default()
        .title(Span::styled(" Changed Files ", title_style))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(ui_colors::BG_HEADER));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.changed_files.is_empty() && state.regenerated_files.is_empty() {
        let empty = Paragraph::new("No file changes detected")
            .style(Style::default().fg(ui_colors::TEXT_DIMMED));
        frame.render_widget(empty, inner);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Show changed files
    for file in &state.changed_files {
        let (icon, color) = match file.change_type {
            FileChangeType::Created => ("●", ui_colors::SUCCESS),
            FileChangeType::Modified => ("●", ui_colors::WARNING),
            FileChangeType::Deleted => ("●", ui_colors::ERROR),
        };

        let path = file
            .path
            .strip_prefix(&state.project_dir)
            .unwrap_or(&file.path);

        // Truncate path if too long
        let path_str = path.display().to_string();
        let max_len = inner.width.saturating_sub(4) as usize;
        let display_path = if path_str.len() > max_len {
            format!("...{}", &path_str[path_str.len().saturating_sub(max_len - 3)..])
        } else {
            path_str
        };

        lines.push(Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(display_path, Style::default().fg(ui_colors::TEXT)),
            Span::styled(
                format!(" ({})", file.change_type),
                Style::default().fg(ui_colors::TEXT_DIMMED),
            ),
        ]));
    }

    // Show regenerated bindings
    if !state.regenerated_files.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "Regenerated bindings:",
            Style::default().fg(brand::BLUE),
        )));

        for file in &state.regenerated_files {
            let path = file.strip_prefix(&state.project_dir).unwrap_or(file);

            // Truncate path if too long
            let path_str = path.display().to_string();
            let max_len = inner.width.saturating_sub(4) as usize;
            let display_path = if path_str.len() > max_len {
                format!("...{}", &path_str[path_str.len().saturating_sub(max_len - 3)..])
            } else {
                path_str
            };

            lines.push(Line::from(vec![
                Span::styled("○", Style::default().fg(brand::BLUE)),
                Span::raw(" "),
                Span::styled(display_path, Style::default().fg(ui_colors::TEXT_MUTED)),
            ]));
        }
    }

    // Limit visible lines
    let visible_lines = inner.height as usize;
    let scroll = state.get_scroll(Panel::Files);
    let total = lines.len();

    let effective_scroll = if total <= visible_lines {
        0
    } else {
        scroll.min(total.saturating_sub(visible_lines))
    };

    let visible: Vec<Line> = lines
        .into_iter()
        .skip(effective_scroll)
        .take(visible_lines)
        .collect();

    let files_widget = Paragraph::new(visible);
    frame.render_widget(files_widget, inner);
}
