use std::process::Command;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use macutil_tui_kit::{
    theme,
    widgets::{centered, panel, with_footer},
    Action, App,
};
use ratatui::{
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
    Frame,
};

const ENTRIES: [(&str, &str, &str); 3] = [
    ("wifi", "macutil-wifi", "scan, connect, hidden networks, power"),
    ("bluetooth", "macutil-bt", "pair, connect, discover, forget"),
    ("tailscale", "macutil-ts", "peers, exit nodes, ping, up/down"),
];

struct Menu {
    list: ListState,
    chosen: Option<&'static str>,
    error: Option<String>,
}

impl App for Menu {
    fn draw(&mut self, f: &mut Frame) {
        let status = self.error.as_deref().map(|e| (e, theme::bad()));
        let body = with_footer(f, &[("↑↓/jk", "move"), ("⏎", "open"), ("q", "quit")], status);
        let area = centered(body, 62, ENTRIES.len() as u16 + 2);
        let items: Vec<ListItem> = ENTRIES
            .iter()
            .map(|(name, _, desc)| {
                ListItem::new(Line::from(vec![Span::raw(format!(" {name:<12}")), Span::styled(*desc, theme::dim())]))
            })
            .collect();
        f.render_stateful_widget(List::new(items).block(panel("macutil")).highlight_style(theme::selected()), area, &mut self.list);
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        let cur = self.list.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.list.select(Some((cur + 1).min(ENTRIES.len() - 1))),
            KeyCode::Up | KeyCode::Char('k') => self.list.select(Some(cur.saturating_sub(1))),
            KeyCode::Enter => {
                self.chosen = Some(ENTRIES[cur].1);
                return Action::Quit;
            }
            _ => {}
        }
        Action::None
    }
}

fn main() -> Result<()> {
    let dir = std::env::current_exe()?.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut menu = Menu { list: ListState::default().with_selected(Some(0)), chosen: None, error: None };
    loop {
        menu.chosen = None;
        macutil_tui_kit::run(&mut menu, Duration::from_secs(60))?;
        let Some(bin) = menu.chosen else { return Ok(()) };
        // Child owns the terminal until it exits, then we return to the menu.
        menu.error = match Command::new(dir.join(bin)).status() {
            Ok(s) if s.success() => None,
            Ok(s) => Some(format!("{bin} exited with {s}")),
            Err(e) => Some(format!("could not start {bin}: {e}")),
        };
    }
}
