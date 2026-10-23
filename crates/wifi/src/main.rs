mod backend;
mod helper;

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::Result;
use backend::{Event, Link, Network};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use macutil_tui_kit::{
    input::{InputEvent, TextInput},
    theme,
    widgets::{panel, prompt, with_footer},
    Action, App,
};
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};

enum Modal {
    Password { ssid: String, input: TextInput },
    Hidden(TextInput),
}

struct Wifi {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    link: Link,
    nets: Vec<Network>,
    list: ListState,
    scanning: bool,
    busy: bool,
    modal: Option<Modal>,
    msg: Option<(String, Style)>,
    pending_ssid: Option<String>,
    seq: u64,
    ticks: u32,
}

impl Wifi {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut w = Self {
            tx,
            rx,
            link: Link::default(),
            nets: vec![],
            list: ListState::default().with_selected(Some(0)),
            scanning: false,
            busy: false,
            modal: None,
            msg: None,
            pending_ssid: None,
            seq: 0,
            ticks: 0,
        };
        w.scan();
        w
    }

    fn scan(&mut self) {
        if !self.scanning {
            self.scanning = true;
            backend::start_scan(&self.tx);
        }
    }

    fn connect(&mut self, ssid: String, pw: Option<String>) {
        self.busy = true;
        self.msg = Some((format!("connecting to {ssid}…"), theme::warn()));
        self.pending_ssid = Some(ssid.clone());
        backend::start_connect(&self.tx, ssid, pw);
    }

    fn drain(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Link(seq, l) if seq >= self.seq => {
                    self.seq = seq;
                    self.link = l;
                }
                Event::Link(..) => {}
                Event::Scan(r) => {
                    self.scanning = false;
                    match r {
                        Ok(n) => self.nets = n,
                        Err(e) => self.msg = Some((e, theme::bad())),
                    }
                }
                Event::Done(r) => {
                    self.busy = false;
                    let pending = self.pending_ssid.take();
                    match r {
                        Ok(m) => self.msg = Some((m, theme::good())),
                        Err(e) => {
                            // A saved-keychain attempt failed; ask for the password.
                            if let Some(ssid) = pending {
                                if self.nets.iter().any(|n| n.ssid == ssid && n.secured) && self.modal.is_none() {
                                    self.modal = Some(Modal::Password { ssid, input: TextInput::new(true) });
                                }
                            }
                            self.msg = Some((e, theme::bad()));
                        }
                    }
                }
            }
        }
    }
}

fn bars(rssi: i64) -> &'static str {
    match rssi {
        r if r >= -55 => "▂▄▆█",
        r if r >= -65 => "▂▄▆ ",
        r if r >= -75 => "▂▄  ",
        _ => "▂   ",
    }
}

impl App for Wifi {
    fn draw(&mut self, f: &mut Frame) {
        let hints = [("↑↓/jk", "move"), ("⏎", "connect"), ("s", "scan"), ("d", "disconnect"), ("h", "hidden"), ("o", "power"), ("q", "quit")];
        let body = with_footer(f, &hints, self.msg.as_ref().map(|(m, s)| (m.as_str(), *s)));
        let [head, main] = Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).areas(body);

        let l = &self.link;
        let (dot, state) = match (&l.ssid, l.powered) {
            (Some(s), _) => (theme::good(), format!("connected to {s}")),
            (None, true) => (theme::warn(), "not connected".to_string()),
            (None, false) => (theme::bad(), "wifi off".to_string()),
        };
        let lines = vec![
            Line::from(vec![Span::styled("● ", dot), Span::styled(state, dot), Span::styled(format!("  {}", l.iface), theme::dim())]),
            Line::styled(
                if l.ssid.is_some() { format!("{} dBm  noise {} dBm  {:.0} Mbps  {}  {}", l.rssi, l.noise, l.rate, l.country, l.bssid.clone().unwrap_or_default()) } else { String::new() },
                theme::dim(),
            ),
        ];
        f.render_widget(Paragraph::new(lines).block(panel("wifi")), head);

        let cur = l.ssid.clone();
        let items: Vec<ListItem> = self
            .nets
            .iter()
            .map(|n| {
                let name = if n.ssid.is_empty() { "(hidden — install the Location helper: scripts/bundle.sh)".to_string() } else { n.ssid.clone() };
                let active = cur.as_deref() == Some(n.ssid.as_str()) && !n.ssid.is_empty();
                ListItem::new(Line::from(vec![
                    Span::styled(if active { "● " } else { "  " }, theme::good()),
                    Span::raw(format!("{name:<32}")),
                    Span::styled(format!("{}  ", bars(n.rssi)), theme::good()),
                    Span::styled(format!("{:>4} dBm  ch{:<3} {}", n.rssi, n.channel, n.security), theme::dim()),
                ]))
            })
            .collect();
        let n = items.len();
        if n > 0 && self.list.selected().is_none_or(|i| i >= n) {
            self.list.select(Some(n - 1));
        }
        let title = if self.scanning { "networks (scanning…)" } else { "networks" };
        f.render_stateful_widget(List::new(items).block(panel(title)).highlight_style(theme::selected()), main, &mut self.list);

        match &self.modal {
            Some(Modal::Password { ssid, input }) => prompt(f, "password", ssid, &input.display()),
            Some(Modal::Hidden(input)) => prompt(f, "hidden network", "enter SSID", &input.display()),
            None => {}
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if let Some(modal) = &mut self.modal {
            let (input, ssid) = match modal {
                Modal::Password { ssid, input } => (input, Some(ssid.clone())),
                Modal::Hidden(input) => (input, None),
            };
            match input.handle(key) {
                InputEvent::Editing => {}
                InputEvent::Cancel => self.modal = None,
                InputEvent::Submit => {
                    let value = input.value.clone();
                    self.modal = None;
                    match ssid {
                        Some(s) => self.connect(s, Some(value)),
                        None if !value.is_empty() => self.connect(value, None),
                        None => {}
                    }
                }
            }
            return Action::None;
        }
        let n = self.nets.len();
        let cur = self.list.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') if n > 0 => self.list.select(Some((cur + 1).min(n - 1))),
            KeyCode::Up | KeyCode::Char('k') => self.list.select(Some(cur.saturating_sub(1))),
            KeyCode::Char('s') | KeyCode::Char('r') => self.scan(),
            KeyCode::Char('d') => backend::start_disconnect(&self.tx),
            KeyCode::Char('o') => backend::start_power(&self.tx, !self.link.powered),
            KeyCode::Char('h') => self.modal = Some(Modal::Hidden(TextInput::new(false))),
            KeyCode::Enter if !self.busy => {
                if let Some(net) = self.nets.get(cur).cloned() {
                    if net.ssid.is_empty() {
                        self.msg = Some(("SSID hidden by macOS; grant Location access".into(), theme::warn()));
                    } else {
                        self.connect(net.ssid, None);
                    }
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
        if self.ticks % 10 == 0 {
            backend::refresh_link(&self.tx);
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--helper") {
        return helper::run(args.get(i + 1).map(String::as_str).unwrap_or("/dev/null"));
    }
    if std::env::args().any(|a| a == "--probe") {
        let (tx, rx) = mpsc::channel();
        backend::start_scan(&tx);
        for _ in 0..2 {
            match rx.recv()? {
                Event::Scan(r) => println!("{r:#?}"),
                Event::Link(_, l) => println!("{l:#?}"),
                Event::Done(_) => {}
            }
        }
        return Ok(());
    }
    macutil_tui_kit::run(&mut Wifi::new(), Duration::from_millis(200))
}
