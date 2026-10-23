mod backend;
mod pairing;

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::Result;
use backend::{Device, Event, PromptKind, Reply};
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

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Paired,
    Nearby,
}

struct PairAsk {
    device: String,
    kind: PromptKind,
    pin: TextInput,
    reply: std::sync::mpsc::Sender<Reply>,
}

struct Bt {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    tab: Tab,
    paired: Vec<Device>,
    nearby: Vec<Device>,
    powered: bool,
    scanning: bool,
    list: ListState,
    msg: Option<(String, Style)>,
    confirm_forget: Option<String>,
    seq: u64,
    ticks: u32,
    ask: Option<PairAsk>,
}

impl Bt {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        backend::refresh(&tx);
        Self {
            tx,
            rx,
            tab: Tab::Paired,
            paired: vec![],
            nearby: vec![],
            powered: false,
            scanning: false,
            list: ListState::default().with_selected(Some(0)),
            msg: None,
            confirm_forget: None,
            seq: 0,
            ticks: 0,
            ask: None,
        }
    }

    fn current(&self) -> &[Device] {
        if self.tab == Tab::Paired { &self.paired } else { &self.nearby }
    }

    fn selected(&self) -> Option<Device> {
        self.list.selected().and_then(|i| self.current().get(i).cloned())
    }

    fn scan(&mut self) {
        if !self.scanning {
            self.scanning = true;
            self.tab = Tab::Nearby;
            self.nearby.clear();
            self.list.select(Some(0));
            backend::discover(&self.tx, 8);
        }
    }

    fn drain(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                // Ignore snapshots older than one already applied.
                Event::Devices { seq, devices, powered } if seq >= self.seq => {
                    self.seq = seq;
                    self.paired = devices;
                    self.powered = powered;
                }
                Event::Devices { .. } => {}
                Event::Discovered(d) => self.nearby = d,
                Event::DiscoveryDone => self.scanning = false,
                Event::PairPrompt { device, kind, reply } => {
                    self.ask = Some(PairAsk { device, kind, pin: TextInput::new(false), reply });
                }
                Event::Done(Ok(m)) => self.msg = Some((m, theme::good())),
                Event::Done(Err(e)) => self.msg = Some((e, theme::bad())),
            }
        }
    }

    fn activate(&mut self) {
        let Some(d) = self.selected() else { return };
        self.msg = Some((format!("working on {}…", d.name), theme::warn()));
        match (self.tab, d.paired, d.connected) {
            (_, true, true) => backend::disconnect(&self.tx, d.address),
            (_, true, false) => backend::connect(&self.tx, d.address),
            (_, false, _) => backend::pair(&self.tx, d.address),
        }
    }
}

impl App for Bt {
    fn draw(&mut self, f: &mut Frame) {
        let hints = [("↑↓/jk", "move"), ("⏎", "connect/pair"), ("tab", "paired/nearby"), ("s", "scan"), ("f", "forget"), ("r", "refresh"), ("q", "quit")];
        let status = match &self.confirm_forget {
            Some(a) => Some((format!("forget {a}? press f again to confirm"), theme::warn())),
            None => self.msg.clone(),
        };
        let body = with_footer(f, &hints, status.as_ref().map(|(m, s)| (m.as_str(), *s)));
        let [head, main] = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(body);

        let (dot, state) = if self.powered { (theme::good(), "powered on") } else { (theme::bad(), "powered off") };
        let connected = self.paired.iter().filter(|d| d.connected).count();
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("● ", dot),
                Span::styled(state, dot),
                Span::styled(format!("   {connected} connected · {} paired", self.paired.len()), theme::dim()),
            ]))
            .block(panel("bluetooth")),
            head,
        );

        let items: Vec<ListItem> = self
            .current()
            .iter()
            .map(|d| {
                let (mark, style) = if d.connected { ("● ", theme::good()) } else if d.paired { ("○ ", theme::base()) } else { ("  ", theme::dim()) };
                let rssi = d.rssi.map(|r| format!("{r} dBm")).unwrap_or_default();
                ListItem::new(Line::from(vec![
                    Span::styled(mark, style),
                    Span::raw(format!("{:<30}", d.name)),
                    Span::styled(format!("{:<10}", d.kind), theme::dim()),
                    Span::styled(format!("{:<19}", d.address), theme::dim()),
                    Span::styled(rssi, theme::dim()),
                ]))
            })
            .collect();
        let n = items.len();
        if n > 0 && self.list.selected().is_none_or(|i| i >= n) {
            self.list.select(Some(n - 1));
        }
        let title = match (self.tab, self.scanning) {
            (Tab::Paired, _) => "paired devices".to_string(),
            (Tab::Nearby, true) => "nearby (scanning…)".to_string(),
            (Tab::Nearby, false) => "nearby".to_string(),
        };
        f.render_stateful_widget(List::new(items).block(panel(title)).highlight_style(theme::selected()), main, &mut self.list);

        if let Some(a) = &self.ask {
            match a.kind {
                PromptKind::Pin => prompt(f, "pairing PIN", &a.device, &a.pin.display()),
                PromptKind::Confirm(code) => {
                    macutil_tui_kit::widgets::message(f, "confirm pairing", &format!("{}\ncode: {code:06}\n\ny = match   n = cancel", a.device))
                }
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if let Some(a) = &mut self.ask {
            let reply = match a.kind {
                PromptKind::Confirm(_) => match key.code {
                    KeyCode::Char('y') | KeyCode::Enter => Some(Reply::Confirm(true)),
                    KeyCode::Char('n') | KeyCode::Esc => Some(Reply::Confirm(false)),
                    _ => None,
                },
                PromptKind::Pin => match a.pin.handle(key) {
                    InputEvent::Submit => Some(Reply::Pin(a.pin.value.clone())),
                    InputEvent::Cancel => Some(Reply::Cancel),
                    InputEvent::Editing => None,
                },
            };
            if let Some(r) = reply {
                let _ = a.reply.send(r);
                self.ask = None;
            }
            return Action::None;
        }
        let n = self.current().len();
        let cur = self.list.selected().unwrap_or(0);
        if key.code != KeyCode::Char('f') {
            self.confirm_forget = None;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') if n > 0 => self.list.select(Some((cur + 1).min(n - 1))),
            KeyCode::Up | KeyCode::Char('k') => self.list.select(Some(cur.saturating_sub(1))),
            KeyCode::Tab => {
                self.tab = if self.tab == Tab::Paired { Tab::Nearby } else { Tab::Paired };
                self.list.select(Some(0));
            }
            KeyCode::Char('s') => self.scan(),
            KeyCode::Char('r') => backend::refresh(&self.tx),
            KeyCode::Enter => self.activate(),
            KeyCode::Char('f') => {
                if let Some(d) = self.selected().filter(|d| d.paired) {
                    if self.confirm_forget.as_deref() == Some(d.name.as_str()) {
                        self.confirm_forget = None;
                        backend::forget(&self.tx, d.address);
                    } else {
                        self.confirm_forget = Some(d.name);
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
        if self.ticks % 15 == 0 {
            backend::refresh(&self.tx);
        }
    }
}

fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--probe") {
        let (tx, rx) = mpsc::channel();
        backend::refresh(&tx);
        if let Event::Devices { devices, powered, .. } = rx.recv()? {
            println!("powered: {powered}\n{devices:#?}");
        }
        if std::env::args().any(|a| a == "--scan") {
            backend::discover(&tx, 4);
            while let Ok(ev) = rx.recv() {
                match ev {
                    Event::Discovered(d) => println!("found {}: {:?}", d.len(), d.iter().map(|d| &d.name).collect::<Vec<_>>()),
                    Event::DiscoveryDone => break,
                    Event::Done(r) => println!("{r:?}"),
                    _ => {}
                }
            }
        }
        return Ok(());
    }
    macutil_tui_kit::run(&mut Bt::new(), Duration::from_millis(200))
}
