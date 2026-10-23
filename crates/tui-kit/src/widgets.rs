use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::theme;

/// Standard rounded panel with an accent title.
pub fn panel<'a>(title: impl Into<String>) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dim())
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title.into(), theme::accent()),
            Span::raw(" "),
        ]))
}

/// Splits the screen into body + one-line footer showing `(key, label)` hints.
pub fn with_footer(f: &mut Frame, hints: &[(&str, &str)], status: Option<(&str, ratatui::style::Style)>) -> Rect {
    let [body, foot] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
    let mut spans = Vec::new();
    for (k, l) in hints {
        spans.push(Span::styled(format!(" {k} "), theme::accent()));
        spans.push(Span::styled(format!("{l}  "), theme::dim()));
    }
    if let Some((msg, style)) = status {
        spans.push(Span::styled(format!("│ {msg}"), style));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), foot);
    body
}

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [v] = Layout::vertical([Constraint::Length(h)]).flex(Flex::Center).areas(area);
    let [r] = Layout::horizontal([Constraint::Length(w)]).flex(Flex::Center).areas(v);
    r
}

/// Modal prompt with a text field.
pub fn prompt(f: &mut Frame, title: &str, text: &str, field: &str) {
    let area = centered(f.area(), 52, 6);
    f.render_widget(Clear, area);
    let body = Paragraph::new(vec![
        Line::styled(text.to_string(), theme::dim()),
        Line::raw(""),
        Line::from(vec![Span::styled("> ", theme::accent()), Span::styled(format!("{field}▏"), theme::base())]),
    ])
    .block(panel(title));
    f.render_widget(body, area);
}

/// Modal message box.
pub fn message(f: &mut Frame, title: &str, text: &str) {
    let h = text.lines().count() as u16 + 2;
    let area = centered(f.area(), 52, h);
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(text.to_string()).block(panel(title)), area);
}
