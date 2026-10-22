//! The status line: a message, a read error, or the input's name and
//! the cursor's line, with the help hint at the right edge.

use crate::app::draw::runes;
use crate::app::prompt::Prompt;
use crate::app::{App, Focus, STDIN};
use crate::input::action::{Action, Command};
use crate::text::ansi::ViewStyle;
use crate::text::buffer::ReadError;
use crate::view::layout::{Layout, Mode as WrapMode, WrapStyle};
use crate::view::screen::{Grid, Terminal};
use crate::view::style::Style;

impl<T: Terminal> App<T> {
    /// Whether the status line has a row: always, unless hidden, and
    /// then for a prompt, a message, help or a read error.
    pub fn status_shown(&self) -> bool {
        self.settings.status_line
            || self.focus != Focus::Viewing
            || self.help.is_some()
            || self.status.is_some()
            || self.read_error().is_some()
    }

    /// The reader's error, else the first a line read hit.
    fn read_error(&self) -> Option<ReadError> {
        let buf = self.view.buf();
        match buf.finished() {
            Some(Err(e)) => Some(e),
            _ => buf.read_error(),
        }
    }

    /// Draws `status_text` on the bottom row, which needs two rows to
    /// exist, unless the line is hidden: in reverse video, or plain
    /// while a prompt is open, so the terminal's cursor shows on it,
    /// where on a reverse row it is the row's own color. "h help",
    /// naming whatever key shows help, is at the right edge unless help
    /// is unbound or up, a prompt is open, or it would come within two
    /// spaces of the text. The : prompt's completions follow it, the one
    /// filled in in the other video and on screen. A prompt wider than
    /// the screen scrolls to keep the cursor on it, at the right edge.
    /// Returns the cell of an open prompt's cursor.
    pub(super) fn draw_status(&self, g: &mut Grid, at: usize) -> Option<(usize, usize)> {
        let (w, h) = (g.width(), g.height());
        if h < 2 || !self.status_shown() {
            return None;
        }
        let prompt = self.prompt();
        let (st, mark) = if prompt.is_some() {
            (Style::default(), Style::default().reverse())
        } else {
            (Style::default().reverse(), Style::default())
        };
        g.fill_row(h - 1, st);
        let text: Vec<char> = self.status_text(w, at).chars().collect();
        let xs = Layout::default().cells(&text);
        let cur = prompt.map(|p| xs[p.cursor()]);
        // At no width the cursor is off screen anyway; the offset must
        // still land on the line.
        let off = cur.map_or(0, |c| (c + 1).saturating_sub(w.max(1)));
        // The first glyph starting on screen: a wide one cut by the
        // edge leaves its cell blank.
        let first = (0..=text.len())
            .find(|&i| xs[i] >= off && (i == text.len() || xs[i + 1] != xs[i]))
            .unwrap_or(text.len());
        let mut width = xs[first] - off + runes(g, xs[first] - off, h - 1, &text[first..], st);
        let cur = cur.map(|c| (c - off, h - 1));
        if let Some((ms, at)) = prompt.and_then(Prompt::listing) {
            let ms: Vec<Vec<char>> = ms.iter().map(|m| m.chars().collect()).collect();
            let cells = |m: &[char]| 2 + Layout::default().cells(m)[m.len()];
            // The list starts late enough for the filled match to end on
            // screen.
            let mut first = 0;
            if let Some(at) = at {
                let mut end = width + ms[..=at].iter().map(|m| cells(m)).sum::<usize>();
                while first < at && end > w {
                    end -= cells(&ms[first]);
                    first += 1;
                }
            }
            for (i, m) in ms.iter().enumerate().skip(first) {
                width += 2;
                let ms = if at == Some(i) { mark } else { st };
                width += runes(g, width, h - 1, m, ms);
            }
        }
        let help = Command {
            action: Action::Help,
            extend: false,
            arg: None,
        };
        if let Some(k) = self.keys.find(&help) {
            let hint: Vec<char> = format!("{k} help").chars().collect();
            if self.focus == Focus::Viewing && self.help.is_none() && width + 2 + hint.len() <= w {
                runes(g, w - hint.len(), h - 1, &hint, st);
            }
        }
        cur
    }

    /// The line the status line names: the cursor's, or the top row's
    /// while the cursor is scrolled off screen, so it says where the
    /// view is.
    pub(super) fn status_at(&mut self) -> usize {
        if self.view.cursor_visible() {
            self.view.cursor().line
        } else {
            self.view.top().line
        }
    }

    fn status_text(&self, width: usize, at: usize) -> String {
        if let Some(p) = self.prompt() {
            return p.line();
        }
        if self.help.is_some() && self.status.is_none() {
            return "help  motion scrolls, / searches, any other key returns".into();
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
        let buf = self.view.buf();
        let mut reading = if buf.finished().is_some() {
            ""
        } else {
            "  reading…"
        }
        .to_string();
        if self.next.is_some() {
            reading.push_str("  reloading…");
        }
        let (k, lines) = buf.part(at);
        let mut rest = String::new();
        if self.names.len() > 1 {
            rest = format!(" {}/{}", k + 1, self.names.len());
        }
        rest.push_str(&format!(
            "  line {}/{}  {mode}{reading}",
            at - lines.start + 1,
            lines.len()
        ));
        let name = fit(
            &self.input_name(at),
            width.saturating_sub(rest.chars().count()),
        );
        format!("{name}{rest}")
    }

    /// What the status line calls the input line `at` came from: its
    /// name, or for stdin the window title in effect at that
    /// line, as a shell prompt sets to the directory.
    fn input_name(&self, at: usize) -> String {
        let buf = self.view.buf();
        let name = self
            .names
            .get(buf.part_at(at))
            .map_or(STDIN, String::as_str);
        if name == STDIN
            && let Some(title) = buf.line(at).title.clone()
            && !title.is_empty()
        {
            return title.to_string();
        }
        name.to_string()
    }
}

/// `name` within `width` cells: whole; else its first directory, "…"
/// and as much of its end as fits, a gap in the middle rather than a
/// head cut off, which reads as a parent; else the file name alone, to
/// clip at the edge.
fn fit(name: &str, width: usize) -> String {
    let cells = |s: &str| {
        let chars: Vec<char> = s.chars().collect();
        Layout::default().cells(&chars)[chars.len()]
    };
    if cells(name) <= width {
        return name.to_string();
    }
    let parts: Vec<&str> = name.split('/').collect();
    for k in 2..parts.len() {
        let s = format!("{}/…/{}", parts[0], parts[k..].join("/"));
        if cells(&s) <= width {
            return s;
        }
    }
    parts[parts.len() - 1].to_string()
}
