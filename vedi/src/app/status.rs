//! The status line: a message, a read error, or the input's name and
//! the cursor's line, with the help hint at the right edge.

use crate::app::draw::runes;
use crate::app::motion::Row;
use crate::app::{App, Focus, STDIN};
use crate::input::action::{Action, Command};
use crate::text::ansi::ViewStyle;
use crate::text::buffer::ReadError;
use crate::view::layout::{Mode as WrapMode, WrapStyle};
use crate::view::screen::{Grid, Terminal};
use crate::view::style::Style;

impl<T: Terminal> App<T> {
    /// Whether the status line has a row: always, unless hidden, and
    /// then for a prompt, a message, help or a read error.
    pub fn status_shown(&self) -> bool {
        self.settings.status_line
            || self.focus != Focus::Viewing
            || self.status.is_some()
            || self.read_error().is_some()
    }

    /// The reader's error, else the first a line read hit.
    fn read_error(&self) -> Option<ReadError> {
        match self.buf.finished() {
            Some(Err(e)) => Some(e),
            _ => self.buf.read_error(),
        }
    }

    /// Scrolls a row when a hidden status line has come back over the
    /// cursor's row: whatever brought it back placed the cursor while
    /// the text still had that row.
    pub(super) fn uncover(&mut self) {
        if self.settings.status_line
            || self.text.is_some()
            || self.start_screen.is_some()
            || !self.status_shown()
            || self.buf.is_empty()
        {
            return;
        }
        let mut p = self.snap(self.top);
        for _ in 0..self.text_rows() {
            let q = self.next_row(p);
            if q == p {
                return;
            }
            p = q;
        }
        if p == self.snap(Row::text(self.cur)) {
            let top = self.snap(self.top);
            self.top = self.next_row(top);
        }
    }

    /// Draws `status_text` in reverse video on the bottom row, which
    /// needs two rows to exist, unless the line is hidden, with "h help",
    /// naming whatever key shows help, at the right edge unless help is
    /// unbound or up, a prompt is open, or it would come within two
    /// spaces of the text. The : prompt's completions follow it, the one
    /// filled in out of reverse video.
    pub(super) fn draw_status(&self, g: &mut Grid) {
        let (w, h) = (g.width(), g.height());
        if h < 2 || !self.status_shown() {
            return;
        }
        let st = Style::default().reverse();
        g.fill_row(h - 1, st);
        let text: Vec<char> = self.status_text().chars().collect();
        let mut width = runes(g, 0, h - 1, &text, st);
        if self.focus == Focus::Commanding
            && let Some((ms, at)) = self.command.listing()
        {
            for (i, m) in ms.iter().enumerate() {
                width += 2;
                let ms = if at == Some(i) { Style::default() } else { st };
                let m: Vec<char> = m.chars().collect();
                width += runes(g, width, h - 1, &m, ms);
            }
        }
        let help = Command {
            action: Action::Help,
            extend: false,
            arg: None,
        };
        if let Some(k) = self.keys.find(&help) {
            let hint: Vec<char> = format!("{k} help").chars().collect();
            if self.focus == Focus::Viewing && width + 2 + hint.len() <= w {
                runes(g, w - hint.len(), h - 1, &hint, st);
            }
        }
    }

    fn status_text(&self) -> String {
        match self.focus {
            Focus::Searching { back: true } => return format!("?{}", self.search.query.text()),
            Focus::Searching { back: false } => return format!("/{}", self.search.query.text()),
            Focus::Commanding => return format!(":{}", self.command.line.text()),
            Focus::Helping if self.status.is_none() => {
                return "help  motion scrolls, / searches, any other key returns".into();
            }
            Focus::Viewing | Focus::Helping => {}
        }
        if let Some(s) = &self.status {
            return s.clone();
        }
        if let Some(e) = self.read_error() {
            return format!("read error: {e}");
        }
        let mut mode = match (self.settings.mode, self.settings.wrap_style) {
            (WrapMode::NoWrap, _) => "nowrap",
            (_, WrapStyle::Word) => "word wrap",
            _ => "wrap",
        }
        .to_string();
        if self.settings.view_style != ViewStyle::Color {
            mode = format!("{mode}  {}", self.settings.view_style);
        }
        let mut reading = if self.buf.finished().is_some() {
            ""
        } else {
            "  reading…"
        }
        .to_string();
        if self.next.is_some() {
            reading.push_str("  reloading…");
        }
        let (k, lines) = self.buf.part(self.cur.line);
        let mut name = self.input_name();
        if self.names.len() > 1 {
            name = format!("{name} {}/{}", k + 1, self.names.len());
        }
        format!(
            "{name}  line {}/{}  {mode}{reading}",
            self.cur.line - lines.start + 1,
            lines.len()
        )
    }

    /// What the status line calls the input the cursor's line came
    /// from: its name, or for stdin the window title in effect at that
    /// line, as a shell prompt sets to the directory.
    fn input_name(&self) -> String {
        let name = self
            .names
            .get(self.buf.part_at(self.cur.line))
            .map_or(STDIN, String::as_str);
        if name == STDIN
            && let Some(title) = self.buf.line(self.cur.line).title.clone()
            && !title.is_empty()
        {
            return title.to_string();
        }
        name.to_string()
    }
}
