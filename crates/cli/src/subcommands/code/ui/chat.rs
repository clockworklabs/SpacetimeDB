//! Chat panel UI component.

use crate::subcommands::code::state::{AppState, MessageRole, Panel};
use crate::subcommands::code::ui::colors::{brand, ui as ui_colors};
use crate::subcommands::code::ui::panel_border_style;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Render the chat panel.
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = state.focused_panel == Panel::Chat;
    let border_style = panel_border_style(is_focused);

    let title_style = if is_focused {
        Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ui_colors::TEXT_DIMMED)
    };

    let block = Block::default()
        .title(Span::styled(" AI Chat ", title_style))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(ui_colors::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.messages.is_empty() && state.current_response.is_empty() {
        let welcome = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to spacetime code!",
                Style::default().fg(brand::GREEN).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Ask me anything about SpacetimeDB:",
                Style::default().fg(ui_colors::TEXT),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  \"How do I add a new table?\"",
                Style::default().fg(ui_colors::TEXT_DIMMED),
            )),
            Line::from(Span::styled(
                "  \"Help me write a reducer\"",
                Style::default().fg(ui_colors::TEXT_DIMMED),
            )),
            Line::from(Span::styled(
                "  \"What's causing this error?\"",
                Style::default().fg(ui_colors::TEXT_DIMMED),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "I can read and edit files in your project.",
                Style::default().fg(ui_colors::TEXT_MUTED),
            )),
            Line::from(Span::styled(
                "Type your message below and press Enter.",
                Style::default().fg(ui_colors::TEXT_MUTED),
            )),
        ]);

        frame.render_widget(welcome, inner);
        return;
    }

    // Build chat content
    let mut lines: Vec<Line> = Vec::new();

    for message in &state.messages {
        // Add sender label
        let (label, label_style) = match message.role {
            MessageRole::User => (
                "You: ",
                Style::default().fg(ui_colors::CHAT_USER).add_modifier(Modifier::BOLD),
            ),
            MessageRole::Assistant => (
                "Assistant: ",
                Style::default().fg(ui_colors::CHAT_ASSISTANT).add_modifier(Modifier::BOLD),
            ),
            MessageRole::System => (
                "System: ",
                Style::default().fg(ui_colors::CHAT_SYSTEM).add_modifier(Modifier::BOLD),
            ),
        };

        lines.push(Line::from(Span::styled(label, label_style)));

        // Add message content with syntax highlighting for code blocks
        for content_line in render_message_content(&message.content) {
            lines.push(content_line);
        }

        // Add empty line between messages
        lines.push(Line::from(""));
    }

    // Add current streaming response if any
    if !state.current_response.is_empty() {
        lines.push(Line::from(Span::styled(
            "Assistant: ",
            Style::default().fg(ui_colors::CHAT_ASSISTANT).add_modifier(Modifier::BOLD),
        )));

        for content_line in render_message_content(&state.current_response) {
            lines.push(content_line);
        }

        // Add typing indicator
        if state.is_ai_responding {
            lines.push(Line::from(Span::styled("▌", Style::default().fg(brand::PURPLE))));
        }
    }

    // Calculate scroll position
    let total_lines = lines.len();
    let visible_lines = inner.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_lines);

    // If following, always show bottom; otherwise use stored scroll position
    let effective_scroll = if state.is_following(Panel::Chat) || total_lines <= visible_lines {
        max_scroll
    } else {
        state.get_scroll(Panel::Chat).min(max_scroll)
    };

    let chat = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll as u16, 0));

    frame.render_widget(chat, inner);

    // Show scroll indicator if content is scrollable and not at bottom
    if total_lines > visible_lines && effective_scroll < max_scroll {
        let lines_below = max_scroll - effective_scroll;
        let indicator = format!(" ↓{} more ", lines_below);

        let indicator_span = Span::styled(
            indicator,
            Style::default().fg(ui_colors::TEXT_DIMMED),
        );
        // Render at bottom-right of inner area
        let indicator_area = Rect {
            x: inner.x + inner.width.saturating_sub(12),
            y: inner.y + inner.height.saturating_sub(1),
            width: 12,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(indicator_span)), indicator_area);
    }
}

/// Render message content with basic markdown support.
fn render_message_content(content: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();

    for line in content.lines() {
        if line.starts_with("```") {
            if in_code_block {
                // End of code block
                in_code_block = false;
                code_lang.clear();
                lines.push(Line::from(Span::styled(
                    "```",
                    Style::default().fg(ui_colors::TEXT_DIMMED),
                )));
            } else {
                // Start of code block
                in_code_block = true;
                code_lang = line.strip_prefix("```").unwrap_or("").to_string();
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(ui_colors::TEXT_DIMMED),
                )));
            }
        } else if in_code_block {
            // Code block content - use green for code
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(brand::GREEN),
            )));
        } else if line.starts_with("# ") {
            // Header
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(brand::PURPLE).add_modifier(Modifier::BOLD),
            )));
        } else if line.starts_with("## ") {
            // Subheader
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(brand::PURPLE),
            )));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            // List item
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(brand::GREEN)),
                Span::styled(line[2..].to_string(), Style::default().fg(ui_colors::TEXT)),
            ]));
        } else if line.starts_with("> ") {
            // Blockquote
            lines.push(Line::from(Span::styled(
                format!("│ {}", &line[2..]),
                Style::default().fg(ui_colors::TEXT_DIMMED),
            )));
        } else {
            // Regular text - handle inline code
            let spans = render_inline_code(line);
            lines.push(Line::from(spans));
        }
    }

    lines
}

/// Render inline code (backticks) in a line.
fn render_inline_code(line: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '`' {
            if in_code {
                // End of inline code
                spans.push(Span::styled(
                    current.clone(),
                    Style::default().fg(brand::GREEN).bg(brand::N6),
                ));
                current.clear();
                in_code = false;
            } else {
                // Start of inline code
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), Style::default().fg(ui_colors::TEXT)));
                    current.clear();
                }
                in_code = true;
            }
        } else {
            current.push(c);
        }
    }

    // Handle any remaining text
    if !current.is_empty() {
        if in_code {
            // Unclosed backtick, treat as regular text with backtick
            spans.push(Span::styled(format!("`{}", current), Style::default().fg(ui_colors::TEXT)));
        } else {
            spans.push(Span::styled(current, Style::default().fg(ui_colors::TEXT)));
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled("".to_string(), Style::default()));
    }

    spans
}
