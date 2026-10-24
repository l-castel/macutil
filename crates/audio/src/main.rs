mod backend;

use std::time::Duration;

use anyhow::Result;
use backend::{Device, Dir};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use macutil_tui_kit::{
    theme,
    widgets::{panel, with_footer},
    Action, App,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};

const BAR: usize = 20;

struct Pane {
    dir: Dir,
    devices: Vec<Device>,
    list: ListState,
}

impl Pane {
    fn new(dir: Dir) -> Self {
        Self { dir, devices: backend::devices(dir), list: ListState::default().with_selected(Some(0)) }
    }

    fn selected(&self) -> Option<&Device> {
        self.list.selected().and_then(|i| self.devices.get(i))
    }

    /// Re-reads the devices, keeping the cursor on the same device if it is still there.
    fn refresh(&mut self) {
        let keep = self.selected().map(|d| d.id);
        self.devices = backend::devices(self.dir);
        let i = keep.and_then(|id| self.devices.iter().position(|d| d.id == id)).or(self.list.selected()).unwrap_or(0);
        self.list.select(Some(i.min(self.devices.len().saturating_sub(1))));
    }
}

struct Audio {
    panes: [Pane; 2],
    focus: usize,
    msg: Option<(String, Style)>,
}

impl Audio {
    fn new() -> Self {
        Self { panes: [Pane::new(Dir::Output), Pane::new(Dir::Input)], focus: 0, msg: None }
    }

    fn pane(&mut self) -> &mut Pane {
        &mut self.panes[self.focus]
    }

    fn refresh(&mut self) {
        self.panes.iter_mut().for_each(Pane::refresh);
    }

    fn report(&mut self, r: Result<String, String>) {
        self.msg = Some(match r {
            Ok(m) => (m, theme::good()),
            Err(e) => (e, theme::bad()),
        });
        self.refresh();
    }

    fn with_selected(&mut self, f: impl FnOnce(Dir, &Device) -> Result<String, String>) {
        let p = self.pane();
        let Some(d) = p.selected() else { return };
        let r = f(p.dir, d);
        self.report(r);
    }

    fn set_volume(&mut self, v: impl FnOnce(f32) -> f32) {
        self.with_selected(|dir, d| {
            let Some(cur) = d.volume else { return Err(format!("{} has no adjustable volume", d.name)) };
            let v = (v(cur) * 100.0).round() / 100.0;
            backend::set_volume(dir, d.id, v).map(|_| format!("{} volume {:.0}%", d.name, v.clamp(0.0, 1.0) * 100.0))
        });
    }

    fn cycle_rate(&mut self) {
        self.with_selected(|_, d| {
            let rates = backend::rates(d.id);
            let cur = d.rate.unwrap_or(0.0);
            let next = rates.iter().copied().find(|&r| r > cur).or(rates.first().copied());
            match next {
                Some(r) if r != cur => backend::set_rate(d.id, r).map(|_| format!("{} at {}", d.name, khz(r))),
                _ => Err(format!("{} supports only one sample rate", d.name)),
            }
        });
    }
}

fn khz(r: f64) -> String {
    format!("{} kHz", r / 1000.0)
}

fn row(d: &Device, width: usize) -> ListItem<'static> {
    let (mark, style) = if d.default { ("● ", theme::good()) } else { ("  ", theme::base()) };
    let muted = d.muted == Some(true);
    let mut spans = vec![Span::styled(mark, style)];
    // Name takes what's left after the fixed columns: kind, bar, percentage, rate, tags.
    let name_w = width.saturating_sub(2 + 12 + BAR + 6 + 12 + 8).clamp(12, 40);
    let mut name: String = d.name.chars().take(name_w).collect();
    if d.name.chars().count() > name_w {
        name.pop();
        name.push('…');
    }
    spans.push(Span::styled(format!("{name:<name_w$}  "), if d.default { theme::accent() } else { theme::base() }));
    spans.push(Span::styled(format!("{:<12}", d.kind), theme::dim()));
    match d.volume {
        Some(v) => {
            let filled = (v * BAR as f32).round() as usize;
            spans.push(Span::styled("━".repeat(filled), if muted { theme::warn() } else { theme::accent() }));
            spans.push(Span::styled("─".repeat(BAR - filled), theme::dim()));
            spans.push(Span::raw(format!(" {:>3.0}% ", v * 100.0)));
        }
        None => spans.push(Span::styled(format!("{:<w$}", "fixed volume", w = BAR + 6), theme::dim())),
    }
    spans.push(Span::styled(format!(" {:<11}", d.rate.map(khz).unwrap_or_default()), theme::dim()));
    if muted {
        spans.push(Span::styled("muted ", theme::warn()));
    }
    if d.system {
        spans.push(Span::styled("alerts", theme::dim()));
    }
    ListItem::new(Line::from(spans))
}

fn draw_pane(f: &mut Frame, area: Rect, p: &mut Pane, focused: bool) {
    let title = if p.dir == Dir::Output { "output" } else { "input" };
    let mut block = panel(title);
    if focused {
        block = block.border_style(theme::accent());
    }
    let width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = if p.devices.is_empty() {
        vec![ListItem::new(Line::styled("  no devices", theme::dim()))]
    } else {
        p.devices.iter().map(|d| row(d, width)).collect()
    };
    let list = List::new(items).block(block);
    let list = if focused && !p.devices.is_empty() { list.highlight_style(theme::selected()) } else { list };
    f.render_stateful_widget(list, area, &mut p.list);
}

impl App for Audio {
    fn draw(&mut self, f: &mut Frame) {
        let hints = [("⏎", "default"), ("←→", "volume"), ("m", "mute"), ("tab", "in/out"), ("a", "alerts"), ("s", "rate"), ("q", "quit")];
        // Status gets its own line: the floating window is too narrow to share it with the hints.
        let body = with_footer(f, &hints, None);
        let h = |p: &Pane| p.devices.len().max(1) as u16 + 2;
        let [out, inp, _, status] = Layout::vertical([
            Constraint::Length(h(&self.panes[0])),
            Constraint::Length(h(&self.panes[1])),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(body);
        if let Some((m, s)) = &self.msg {
            f.render_widget(Paragraph::new(Line::styled(format!(" {m}"), *s)), status);
        }
        let focus = self.focus;
        for (i, (p, area)) in self.panes.iter_mut().zip([out, inp]).enumerate() {
            draw_pane(f, area, p, i == focus);
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let step = if shift { 0.01 } else { 0.05 };
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => {
                let p = self.pane();
                let n = p.devices.len();
                let cur = p.list.selected().unwrap_or(0);
                if cur + 1 < n {
                    p.list.select(Some(cur + 1));
                } else if self.focus == 0 {
                    self.focus = 1;
                    self.pane().list.select(Some(0));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let p = self.pane();
                let cur = p.list.selected().unwrap_or(0);
                if cur > 0 {
                    p.list.select(Some(cur - 1));
                } else if self.focus == 1 {
                    self.focus = 0;
                    let p = self.pane();
                    p.list.select(Some(p.devices.len().saturating_sub(1)));
                }
            }
            KeyCode::Tab | KeyCode::BackTab => self.focus = 1 - self.focus,
            KeyCode::Enter | KeyCode::Char(' ') => self.with_selected(|dir, d| {
                let what = if dir == Dir::Output { "output" } else { "input" };
                backend::set_default(dir, d.id).map(|_| format!("{what}: {}", d.name))
            }),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Char('+') | KeyCode::Char('=') => {
                self.set_volume(|v| v + step)
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Char('-') => self.set_volume(|v| v - step),
            KeyCode::Char(c @ '0'..='9') => {
                let v = if c == '0' { 1.0 } else { c.to_digit(10).unwrap() as f32 / 10.0 };
                self.set_volume(|_| v);
            }
            KeyCode::Char('m') => self.with_selected(|dir, d| {
                backend::toggle_mute(dir, d.id).map(|m| format!("{} {}", d.name, if m { "muted" } else { "unmuted" }))
            }),
            KeyCode::Char('a') if self.focus == 0 => {
                self.with_selected(|_, d| backend::set_system(d.id).map(|_| format!("alerts and sound effects: {}", d.name)))
            }
            KeyCode::Char('s') => self.cycle_rate(),
            KeyCode::Char('r') => {
                self.msg = None;
                self.refresh();
            }
            _ => {}
        }
        Action::None
    }

    fn on_tick(&mut self) {
        // Picks up devices plugged in or changed elsewhere (menu bar, Sound settings, volume keys).
        self.refresh();
    }
}

fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--probe") {
        for dir in [Dir::Output, Dir::Input] {
            println!("{dir:?}");
            for d in backend::devices(dir) {
                println!("  {d:?}\n    rates: {:?}", backend::rates(d.id));
            }
        }
        return Ok(());
    }
    macutil_tui_kit::run(&mut Audio::new(), Duration::from_millis(500))
}
