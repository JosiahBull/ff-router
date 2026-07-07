//! The full-screen wizard: pick the default profile, enter globs per other
//! profile, review the generated config.

use std::io;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::config;
use crate::discover::Profile;

pub enum Outcome {
    Install(String),
    Cancelled,
}

enum Step {
    Default,
    Globs,
    Review,
}

pub struct Wizard {
    profiles: Vec<Profile>,
    globs: Vec<String>, // parallel to `profiles`
    default_idx: usize,
    step: Step,
    list: ListState,
    edit_order: Vec<usize>, // non-default profile indices, in edit order
    edit_pos: usize,
    input: Vec<char>,
    cursor: usize,
}

impl Wizard {
    pub fn new(profiles: Vec<Profile>) -> Self {
        let globs = vec![String::new(); profiles.len()];
        let mut list = ListState::default();
        list.select(Some(0));
        Self {
            profiles,
            globs,
            default_idx: 0,
            step: Step::Default,
            list,
            edit_order: Vec::new(),
            edit_pos: 0,
            input: Vec::new(),
            cursor: 0,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<Outcome> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if let Some(outcome) = self.handle(key) {
                return Ok(outcome);
            }
        }
    }

    fn handle(&mut self, key: KeyEvent) -> Option<Outcome> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(Outcome::Cancelled);
        }
        match self.step {
            Step::Default => match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                KeyCode::Enter => self.confirm_default(),
                KeyCode::Esc | KeyCode::Char('q') => return Some(Outcome::Cancelled),
                _ => {}
            },
            Step::Globs => match key.code {
                KeyCode::Enter => self.confirm_globs(),
                KeyCode::Esc => self.step = Step::Default,
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.input.remove(self.cursor);
                    }
                }
                KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
                KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.len()),
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.input.len(),
                KeyCode::Char(c) => {
                    self.input.insert(self.cursor, c);
                    self.cursor += 1;
                }
                _ => {}
            },
            Step::Review => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    return Some(Outcome::Install(self.config()));
                }
                KeyCode::Char('b') => self.step = Step::Default,
                KeyCode::Esc | KeyCode::Char('q') => return Some(Outcome::Cancelled),
                _ => {}
            },
        }
        None
    }

    fn move_selection(&mut self, delta: isize) {
        let n = self.profiles.len() as isize;
        let cur = self.list.selected().unwrap_or(0) as isize;
        self.list.select(Some((cur + delta).rem_euclid(n) as usize));
    }

    fn confirm_default(&mut self) {
        self.default_idx = self.list.selected().unwrap_or(0);
        self.edit_order = (0..self.profiles.len())
            .filter(|&i| i != self.default_idx)
            .collect();
        self.edit_pos = 0;
        if self.edit_order.is_empty() {
            self.step = Step::Review;
        } else {
            self.load_input();
            self.step = Step::Globs;
        }
    }

    fn confirm_globs(&mut self) {
        let idx = self.edit_order[self.edit_pos];
        self.globs[idx] = self.input.iter().collect();
        if self.edit_pos + 1 < self.edit_order.len() {
            self.edit_pos += 1;
            self.load_input();
        } else {
            self.step = Step::Review;
        }
    }

    fn load_input(&mut self) {
        self.input = self.globs[self.edit_order[self.edit_pos]].chars().collect();
        self.cursor = self.input.len();
    }

    fn config(&self) -> String {
        config::gen_config(&self.profiles, self.default_idx, &self.globs)
    }

    // --- rendering -------------------------------------------------------

    fn render(&mut self, frame: &mut Frame) {
        let [body, footer] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(frame.area());
        match self.step {
            Step::Default => self.render_default(frame, body),
            Step::Globs => self.render_globs(frame, body),
            Step::Review => self.render_review(frame, body),
        }
        let help = match self.step {
            Step::Default => "↑/↓ move · Enter: set as default · q/Esc quit",
            Step::Globs => "type globs · Enter: next · Esc: back",
            Step::Review => "Enter/y: install · b: back · q/Esc: cancel",
        };
        frame.render_widget(
            Paragraph::new(help).block(Block::default().borders(Borders::ALL).title(" Keys ")),
            footer,
        );
    }

    fn render_default(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .profiles
            .iter()
            .map(|p| ListItem::new(format!("{:<24} {}", p.name, p.dir)))
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Which profile is the DEFAULT? (used when no rule matches) "),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("● ");
        frame.render_stateful_widget(list, area, &mut self.list);
    }

    fn render_globs(&self, frame: &mut Frame, area: Rect) {
        let profile = &self.profiles[self.edit_order[self.edit_pos]];
        let [hint, entry, _] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .areas(area);

        let lines = vec![
            Line::from("Space-separated glob patterns to open in this profile."),
            Line::from("Examples:  *://*.atlassian.net/*   *partly.com/*   *.{slack,notion}.com/*"),
            Line::from("Leave blank to skip this profile."),
        ];
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", profile.name)),
            ),
            hint,
        );

        let text: String = self.input.iter().collect();
        frame.render_widget(
            Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(format!(
                " globs ({}/{}) ",
                self.edit_pos + 1,
                self.edit_order.len()
            ))),
            entry,
        );

        let max_x = entry.x + entry.width.saturating_sub(2);
        let cursor_x = (entry.x + 1 + self.cursor as u16).min(max_x);
        frame.set_cursor_position((cursor_x, entry.y + 1));
    }

    fn render_review(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Paragraph::new(self.config())
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Review ~/.ff-router.toml "),
                ),
            area,
        );
    }
}
