use crossterm::event::{KeyCode, KeyEvent};

/// Single-line text input (optionally masked) for passwords, names, auth keys.
#[derive(Default)]
pub struct TextInput {
    pub value: String,
    pub masked: bool,
}

pub enum InputEvent {
    Editing,
    Submit,
    Cancel,
}

impl TextInput {
    pub fn new(masked: bool) -> Self {
        Self { value: String::new(), masked }
    }

    pub fn handle(&mut self, key: KeyEvent) -> InputEvent {
        match key.code {
            KeyCode::Enter => return InputEvent::Submit,
            KeyCode::Esc => return InputEvent::Cancel,
            KeyCode::Backspace => {
                self.value.pop();
            }
            KeyCode::Char(c) => self.value.push(c),
            _ => {}
        }
        InputEvent::Editing
    }

    pub fn display(&self) -> String {
        if self.masked {
            "•".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        }
    }
}
