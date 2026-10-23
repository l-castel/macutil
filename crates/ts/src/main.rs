mod backend;

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::Result;
use backend::{Peer, Status};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use macutil_tui_kit::{
    input::{InputEvent, TextInput},
    theme,
    widgets::{panel, prompt, with_footer},
    Action, App,
};
use ratatui::{
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Peers,
    ExitNodes,
}

enum Event {
    Status(Result<Status, String>),
    Done(Result<String, String>),
}

struct Ts {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    refreshing: bool,
    last_error: Option<String>,
    ticks: u32,
    status: Option<Status>,
    tab: Tab,
    list: ListState,
    filter: Option<TextInput>,
    filter_text: String,
    msg: Option<(String, ratatui::style::Style)>,
}

impl Ts {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut s = Self {
            tx,
            rx,
            refreshing: false,
            last_error: None,
            ticks: 0,
            status: None,
            tab: Tab::Peers,
            list: ListState::default().with_selected(Some(0)),
            filter: None,
            filter_text: String::new(),
            msg: None,
        };
        s.refresh();
        s
    }

    /// Fetches status off the UI thread; at most one fetch in flight.
    fn refresh(&mut self) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Status(backend::status().map_err(|e| e.to_string())));
        });
    }

    /// Runs a blocking tailscale command off the UI thread (`up` can wait on browser login).
    fn run(&mut self, label: &str, job: impl FnOnce() -> anyhow::Result<String> + Send + 'static) {
        self.msg = Some((label.to_string(), theme::warn()));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Done(job().map_err(|e| e.to_string())));
        });
    }

    fn drain(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Status(r) => {
                    self.refreshing = false;
                    match r {
                        Ok(st) => {
                            self.status = Some(st);
                            self.last_error = None;
                        }
                        Err(e) => {
                            self.last_error = Some(e.clone());
                            self.msg = Some((e, theme::bad()));
                        }
                    }
                }
                Event::Done(r) => {
                    self.msg = Some(match r {
                        Ok(m) => (m, theme::good()),
                        Err(e) => (e, theme::bad()),
                    });
                    self.refresh();
                }
            }
        }
    }

    fn err(&mut self, e: anyhow::Error) {
        self.msg = Some((e.to_string(), theme::bad()));
    }

    fn ok(&mut self, m: impl Into<String>) {
        self.msg = Some((m.into(), theme::good()));
    }

    fn visible(&self) -> Vec<&Peer> {
        let q = self.filter_text.to_lowercase();
        self.status
            .iter()
            .flat_map(|s| s.peers.iter())
            .filter(|p| self.tab == Tab::Peers || p.is_exit_option)
            .filter(|p| q.is_empty() || p.label.to_lowercase().contains(&q) || p.name.to_lowercase().contains(&q) || p.country.to_lowercase().contains(&q) || p.country_code.to_lowercase() == q || p.ip.contains(&q))
            .collect()
    }

    fn selected(&self) -> Option<Peer> {
        let v = self.visible();
        self.list.selected().and_then(|i| v.get(i).map(|p| (*p).clone()))
    }

    fn toggle_up(&mut self) {
        if self.status.as_ref().is_some_and(|s| s.running()) {
            self.run("disconnecting…", || backend::down().map(|_| "disconnected".into()));
        } else {
            self.run("connecting… (complete login in your browser if prompted)", || backend::up().map(|_| "connected".into()));
        }
    }

    fn toggle_exit(&mut self) {
        let Some(p) = self.selected() else { return };
        if !p.is_exit_option {
            self.msg = Some(("not an exit node".into(), theme::warn()));
            return;
        }
        if p.is_exit_active {
            self.run("clearing exit node…", || backend::set_exit_node("").map(|_| "exit node cleared".into()));
        } else {
            // Never pass an empty target: that would clear the exit node instead of setting it.
            let target = if p.dns.is_empty() { p.ip.clone() } else { p.dns.clone() };
            if target.is_empty() {
                self.msg = Some(("peer has no address".into(), theme::bad()));
                return;
            }
            let name = p.label.clone();
            self.run("setting exit node…", move || backend::set_exit_node(&target).map(|_| format!("exit node: {name}")));
        }
    }
}

impl App for Ts {
    fn draw(&mut self, f: &mut Frame) {
        let hints = [("↑↓/jk", "move"), ("tab", "view"), ("u", "up/down"), ("e", "exit node"), ("p", "ping"), ("y", "copy ip"), ("/", "filter"), ("r", "refresh"), ("q", "quit")];
        let body = with_footer(f, &hints, self.msg.as_ref().map(|(m, s)| (m.as_str(), *s)));
        let [head, main] = Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).areas(body);

        // Header: connection state
        let (state, tailnet, me) = match &self.status {
            Some(s) => (s.state.clone(), s.tailnet.clone(), s.me.clone()),
            None => ("unavailable".into(), self.last_error.clone().unwrap_or_default(), None),
        };
        let st_style = if state == "Running" { theme::good() } else { theme::warn() };
        let exit = self.status.iter().flat_map(|s| s.peers.iter()).find(|p| p.is_exit_active).map(|p| p.label.clone());
        let login = self.status.as_ref().map(|s| s.auth_url.clone()).filter(|u| !u.is_empty());
        let lines = vec![
            Line::from(vec![Span::styled("● ", st_style), Span::styled(state, st_style), Span::styled(format!("  {tailnet}"), theme::dim()), Span::styled(login.map(|u| format!("  login: {u}")).unwrap_or_default(), theme::warn())]),
            Line::from(vec![
                Span::styled(me.as_ref().map(|m| m.name.clone()).unwrap_or_default(), theme::base()),
                Span::styled(format!("  {}", me.as_ref().map(|m| m.ip.clone()).unwrap_or_default()), theme::dim()),
                Span::styled(format!("   exit: {}", exit.unwrap_or_else(|| "none".into())), theme::dim()),
            ]),
        ];
        f.render_widget(Paragraph::new(lines).block(panel("tailscale")).wrap(Wrap { trim: true }), head);

        // Peer list
        let title = match self.tab {
            Tab::Peers => "peers",
            Tab::ExitNodes => "exit nodes",
        };
        let title = if self.filter_text.is_empty() { title.to_string() } else { format!("{title} /{}", self.filter_text) };
        let items: Vec<ListItem> = self
            .visible()
            .iter()
            .map(|p| {
                let dot = if p.online { Span::styled("● ", theme::good()) } else { Span::styled("○ ", theme::dim()) };
                let mark = if p.is_exit_active { Span::styled(" [exit]", theme::accent()) } else { Span::raw("") };
                ListItem::new(Line::from(vec![
                    dot,
                    Span::raw(format!("{:<24}", p.label)),
                    Span::styled(format!("{:<16}", p.country), theme::dim()),
                    Span::styled(format!("{:<16}", p.ip), theme::dim()),
                    Span::styled(format!("{:<8}", p.os), theme::dim()),
                    Span::styled(p.path.clone(), theme::dim()),
                    mark,
                ]))
            })
            .collect();
        let n = items.len();
        if let Some(i) = self.list.selected() {
            if n > 0 && i >= n {
                self.list.select(Some(n - 1));
            }
        }
        f.render_stateful_widget(List::new(items).block(panel(title)).highlight_style(theme::selected()), main, &mut self.list);

        if let Some(inp) = &self.filter {
            prompt(f, "filter", "match name or ip", &inp.display());
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if let Some(inp) = &mut self.filter {
            match inp.handle(key) {
                InputEvent::Editing => self.filter_text = inp.value.clone(),
                InputEvent::Submit => self.filter = None,
                InputEvent::Cancel => {
                    self.filter = None;
                    self.filter_text.clear();
                }
            }
            return Action::None;
        }
        let n = self.visible().len();
        let cur = self.list.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') if n > 0 => self.list.select(Some((cur + 1).min(n - 1))),
            KeyCode::Up | KeyCode::Char('k') => self.list.select(Some(cur.saturating_sub(1))),
            KeyCode::Tab => {
                self.tab = if self.tab == Tab::Peers { Tab::ExitNodes } else { Tab::Peers };
                self.list.select(Some(0));
            }
            KeyCode::Char('u') => self.toggle_up(),
            KeyCode::Char('e') => self.toggle_exit(),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('/') => self.filter = Some(TextInput::new(false)),
            KeyCode::Char('y') => {
                if let Some(p) = self.selected() {
                    match backend::copy(&p.ip) {
                        Ok(()) => self.ok(format!("copied {}", p.ip)),
                        Err(e) => self.err(e),
                    }
                }
            }
            KeyCode::Char('p') => {
                if let Some(p) = self.selected() {
                    let (ip, name) = (p.ip.clone(), p.name.clone());
                    self.run("pinging…", move || backend::ping(&ip).map(|r| format!("{name}: {r}")));
                }
            }
            _ => {}
        }
        Action::None
    }

    fn on_poll(&mut self) {
        self.drain();
    }

    fn on_tick(&mut self) {
        self.ticks += 1;
        if self.ticks % 25 == 0 {
            self.refresh();
        }
    }
}

fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--probe") {
        let st = backend::status()?;
        println!("{} {} peers={}", st.state, st.tailnet, st.peers.len());
        for p in st.peers.iter().take(5) {
            println!("{} | {} | {} | {}", p.label, p.country, p.ip, p.online);
        }
        return Ok(());
    }
    macutil_tui_kit::run(&mut Ts::new(), Duration::from_millis(200))
}
