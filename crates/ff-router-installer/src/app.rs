//! The full-screen wizard: pick the default profile, enter globs per other
//! profile, review the config, then step through the install plan
//! action-by-action (with a diff-powered conflict resolver).

use std::io;
use std::path::PathBuf;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::discover::Profile;
use crate::{config, diff, plan};

pub enum Outcome {
    Install {
        plan: Vec<plan::Decided>,
        warnings: Vec<String>,
    },
    Cancelled,
}

enum Step {
    Default,
    Globs,
    Review,
    Plan,
}

pub struct Wizard {
    root: PathBuf,
    profiles: Vec<Profile>,
    globs: Vec<String>, // parallel to `profiles`
    default_idx: usize,
    step: Step,
    list: ListState,
    edit_order: Vec<usize>, // non-default profile indices, in edit order
    edit_pos: usize,
    input: Vec<char>,
    cursor: usize,
    review_scroll: u16,
    review_max_scroll: u16,
    review_page: u16,
    // Plan phase.
    actions: Vec<plan::Action>,
    plan_pos: usize,
    decisions: Vec<bool>,
    warnings: Vec<String>,
    conflict: plan::Conflict,
    diff_lines: Vec<Line<'static>>,
    diff_open: bool,
    diff_scroll: u16,
    diff_max_scroll: u16,
    diff_page: u16,
}

impl Wizard {
    pub fn new(profiles: Vec<Profile>, root: PathBuf) -> Self {
        let globs = vec![String::new(); profiles.len()];
        let mut list = ListState::default();
        list.select(Some(0));
        Self {
            root,
            profiles,
            globs,
            default_idx: 0,
            step: Step::Default,
            list,
            edit_order: Vec::new(),
            edit_pos: 0,
            input: Vec::new(),
            cursor: 0,
            review_scroll: 0,
            review_max_scroll: 0,
            review_page: 1,
            actions: Vec::new(),
            plan_pos: 0,
            decisions: Vec::new(),
            warnings: Vec::new(),
            conflict: plan::Conflict::None,
            diff_lines: Vec::new(),
            diff_open: false,
            diff_scroll: 0,
            diff_max_scroll: 0,
            diff_page: 1,
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
                KeyCode::Enter | KeyCode::Char('y') => self.start_plan(),
                KeyCode::Char('b') => self.step = Step::Default,
                KeyCode::Esc | KeyCode::Char('q') => return Some(Outcome::Cancelled),
                KeyCode::Up | KeyCode::Char('k') => {
                    self.review_scroll = self.review_scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.review_scroll = (self.review_scroll + 1).min(self.review_max_scroll);
                }
                KeyCode::PageUp => {
                    self.review_scroll = self.review_scroll.saturating_sub(self.review_page);
                }
                KeyCode::PageDown => {
                    self.review_scroll =
                        (self.review_scroll + self.review_page).min(self.review_max_scroll);
                }
                KeyCode::Home | KeyCode::Char('g') => self.review_scroll = 0,
                KeyCode::End | KeyCode::Char('G') => self.review_scroll = self.review_max_scroll,
                _ => {}
            },
            Step::Plan => return self.handle_plan(key),
        }
        None
    }

    fn handle_plan(&mut self, key: KeyEvent) -> Option<Outcome> {
        if self.diff_open {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => self.diff_open = false,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.diff_scroll = self.diff_scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.diff_scroll = (self.diff_scroll + 1).min(self.diff_max_scroll);
                }
                KeyCode::PageUp => {
                    self.diff_scroll = self.diff_scroll.saturating_sub(self.diff_page);
                }
                KeyCode::PageDown => {
                    self.diff_scroll =
                        (self.diff_scroll + self.diff_page).min(self.diff_max_scroll);
                }
                KeyCode::Home | KeyCode::Char('g') => self.diff_scroll = 0,
                KeyCode::End | KeyCode::Char('G') => self.diff_scroll = self.diff_max_scroll,
                _ => {}
            }
            return None;
        }

        let has_conflict = !matches!(self.conflict, plan::Conflict::None);
        let can_compare = matches!(self.conflict, plan::Conflict::Text { .. });
        match key.code {
            KeyCode::Char('a') | KeyCode::Esc => Some(Outcome::Cancelled),
            KeyCode::Char('s') => self.decide(false),
            KeyCode::Char('c') if can_compare => {
                self.diff_open = true;
                self.diff_scroll = 0;
                None
            }
            KeyCode::Char('r') if has_conflict => self.decide(true),
            KeyCode::Enter | KeyCode::Char('y') if !has_conflict => self.decide(true),
            _ => None,
        }
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
            self.enter_review();
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
            self.enter_review();
        }
    }

    fn enter_review(&mut self) {
        self.review_scroll = 0;
        self.step = Step::Review;
    }

    fn load_input(&mut self) {
        self.input = self.globs[self.edit_order[self.edit_pos]].chars().collect();
        self.cursor = self.input.len();
    }

    fn config(&self) -> String {
        config::gen_config(&self.profiles, self.default_idx, &self.globs)
    }

    fn start_plan(&mut self) {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        self.warnings = config::glob_warnings(&self.globs);
        self.actions = plan::build(&self.root, &home, self.config());
        self.plan_pos = 0;
        self.decisions = Vec::new();
        self.enter_action();
        self.step = Step::Plan;
    }

    /// Prepare conflict info (and any diff) for the current action.
    fn enter_action(&mut self) {
        self.diff_open = false;
        self.diff_scroll = 0;
        self.conflict = self.actions[self.plan_pos].conflict();
        self.diff_lines = match &self.conflict {
            plan::Conflict::Text {
                existing, proposed, ..
            } => diff::lines(existing, proposed),
            _ => Vec::new(),
        };
    }

    /// Record a decision for the current action and advance; produce the final
    /// outcome once every action has been decided.
    fn decide(&mut self, apply: bool) -> Option<Outcome> {
        self.decisions.push(apply);
        self.plan_pos += 1;
        if self.plan_pos < self.actions.len() {
            self.enter_action();
            return None;
        }
        let plan = std::mem::take(&mut self.actions)
            .into_iter()
            .zip(std::mem::take(&mut self.decisions))
            .map(|(action, apply)| plan::Decided { action, apply })
            .collect();
        Some(Outcome::Install {
            plan,
            warnings: std::mem::take(&mut self.warnings),
        })
    }

    // --- rendering -------------------------------------------------------

    fn render(&mut self, frame: &mut Frame) {
        let [body, footer] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(frame.area());
        match self.step {
            Step::Default => self.render_default(frame, body),
            Step::Globs => self.render_globs(frame, body),
            Step::Review => self.render_review(frame, body),
            Step::Plan => self.render_plan(frame, body),
        }
        let help = self.help();
        frame.render_widget(
            Paragraph::new(help).block(Block::default().borders(Borders::ALL).title(" Keys ")),
            footer,
        );
    }

    fn help(&self) -> String {
        match self.step {
            Step::Default => "↑/↓ move · Enter: set as default · q/Esc quit".into(),
            Step::Globs => "type globs · Enter: next · Esc: back".into(),
            Step::Review if self.review_max_scroll > 0 => {
                "Enter/y: install · b: back · q/Esc: cancel · ↑/↓ j/k g/G: scroll".into()
            }
            Step::Review => "Enter/y: install · b: back · q/Esc: cancel".into(),
            Step::Plan if self.diff_open => "↑/↓ j/k g/G: scroll · Esc/Enter/q: close".into(),
            Step::Plan => match &self.conflict {
                plan::Conflict::None => "Enter: apply · s: skip · a/Esc: abort".into(),
                plan::Conflict::Text { .. } => {
                    "c: compare · r: replace · s: skip · a/Esc: abort".into()
                }
                plan::Conflict::Exists(_) => "r: replace · s: skip · a/Esc: abort".into(),
            },
        }
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
            Line::from("Examples:  *://*.atlassian.net/*   *partly.com/*   *://github.com/partly*"),
            Line::from("Leave blank to skip this profile. ('*' matches any run of characters.)"),
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

    fn render_review(&mut self, frame: &mut Frame, area: Rect) {
        let config = self.config();
        let total = config.lines().count() as u16;
        let viewport = area.height.saturating_sub(2); // account for the borders
        self.review_page = viewport.max(1);
        self.review_max_scroll = total.saturating_sub(viewport);
        self.review_scroll = self.review_scroll.min(self.review_max_scroll);

        let mut block = Block::default()
            .borders(Borders::ALL)
            .title(" Review ~/.ff-router.toml ");
        if self.review_max_scroll > 0 {
            let up = if self.review_scroll > 0 { '↑' } else { ' ' };
            let down = if self.review_scroll < self.review_max_scroll {
                '↓'
            } else {
                ' '
            };
            block = block.title_bottom(
                Line::from(format!(" more {up}{down} · scroll: ↑/↓ j/k g/G ")).right_aligned(),
            );
        }
        frame.render_widget(
            Paragraph::new(config)
                .scroll((self.review_scroll, 0))
                .block(block),
            area,
        );
    }

    fn render_plan(&mut self, frame: &mut Frame, area: Rect) {
        if self.diff_open {
            self.render_diff(frame, area);
            return;
        }

        let action = &self.actions[self.plan_pos];
        let mut lines = vec![
            Line::from(format!(
                "Action {} of {}",
                self.plan_pos + 1,
                self.actions.len()
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("I am going to: "),
                Span::styled(
                    action.summary(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(format!("    {}", action.detail())),
        ];
        let target = match &self.conflict {
            plan::Conflict::None => None,
            plan::Conflict::Exists(p) | plan::Conflict::Text { path: p, .. } => Some(p),
        };
        if let Some(path) = target {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "⚠ {} already exists — choose what to do.",
                    plan::home_relative(path)
                ),
                Style::default().fg(Color::Yellow),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Install plan "),
            ),
            area,
        );
    }

    fn render_diff(&mut self, frame: &mut Frame, area: Rect) {
        let total = self.diff_lines.len() as u16;
        let viewport = area.height.saturating_sub(2);
        self.diff_page = viewport.max(1);
        self.diff_max_scroll = total.saturating_sub(viewport);
        self.diff_scroll = self.diff_scroll.min(self.diff_max_scroll);

        let mut block = Block::default()
            .borders(Borders::ALL)
            .title(" Compare  (–red existing · +green proposed) ");
        if self.diff_max_scroll > 0 {
            let up = if self.diff_scroll > 0 { '↑' } else { ' ' };
            let down = if self.diff_scroll < self.diff_max_scroll {
                '↓'
            } else {
                ' '
            };
            block = block.title_bottom(Line::from(format!(" more {up}{down} ")).right_aligned());
        }
        frame.render_widget(
            Paragraph::new(Text::from(self.diff_lines.clone()))
                .scroll((self.diff_scroll, 0))
                .block(block),
            area,
        );
    }
}
