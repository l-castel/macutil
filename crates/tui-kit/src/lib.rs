//! Shared terminal UI kit for the macutil apps: theme, event loop, common widgets.

pub mod input;
pub mod theme;
pub mod widgets;

use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::Frame;

pub enum Action {
    None,
    Quit,
}

pub trait App {
    fn draw(&mut self, f: &mut Frame);
    fn on_key(&mut self, key: KeyEvent) -> Action;
    /// Called before every frame; the place to apply results from background threads.
    fn on_poll(&mut self) {}
    fn on_tick(&mut self) {}
}

/// Runs `app` until it asks to quit, restoring the terminal on exit or panic.
pub fn run(app: &mut impl App, tick: Duration) -> Result<()> {
    let mut term = ratatui::init();
    let res = (|| -> Result<()> {
        let mut last = Instant::now();
        loop {
            theme::refresh();
            app.on_poll();
            term.draw(|f| {
                f.render_widget(ratatui::widgets::Block::new().style(theme::base()), f.area());
                app.draw(f)
            })?;
            let timeout = tick.saturating_sub(last.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        if let Action::Quit = app.on_key(k) {
                            return Ok(());
                        }
                    }
                }
            }
            if last.elapsed() >= tick {
                app.on_tick();
                last = Instant::now();
            }
        }
    })();
    ratatui::restore();
    res
}
