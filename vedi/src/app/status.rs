//! The status line: a message, a read error, or the input's name and
//! the cursor's line, with the help hint at the right edge.

use crate::app::draw::runes;
use crate::app::prompt::Prompt;
use crate::app::views::Role;
use crate::app::{App, Flow, Focus, Kind, STDIN};
use crate::input::action::{Action, Command};
use crate::input::setting;
use crate::text::ansi::ViewStyle;
use crate::text::buffer::ReadError;
use crate::view::layout::{self, Layout, Mode as WrapMode, WrapStyle};
use crate::view::screen::{Grid, Terminal};
use crate::view::style::Style;

/// The row the status line names: a line, or the end row past it.
#[derive(Copy, Clone)]
pub(super) struct At {
    line: usize,
    past: bool,
}

/// What the status line shows, the first of these that applies.
enum Row<'a> {
    Prompt(&'a Prompt),
    /// How help is left, unless something was said under it.
    HelpHint,
    Message(&'a str),
    ReadError(ReadError),
    /// The input's name and the cursor's line.
    Summary,
    /// Nothing, and no row: `status_line no` with nothing to say.
    Hidden,
}

/// Whether the summary row shows `verb`'s value, so cycle needn't say
/// it.
pub(super) fn summarizes(verb: &str) -> bool {
    setting::verb(verb).is_some_and(|v| v.summarized)
}

impl<T: Terminal> App<T> {
    fn row(&self) -> Row<'_> {
        if let Some(p) = self.prompt() {
            return Row::Prompt(p);
        }
        if self.views.role() == Role::Help && self.status.is_none() {
            return Row::HelpHint;
        }
        if let Some(s) = &self.status {
            return Row::Message(s);
        }
        if let Some(e) = self.read_error() {
            return Row::ReadError(e);
        }
        if self.settings.status_line {
            Row::Summary
        } else {
            Row::Hidden
        }
    }

    /// Whether the status line has a row.
    pub fn status_shown(&self) -> bool {
        !matches!(self.row(), Row::Hidden)
    }

    /// Puts what a handler said on the status line, where it stays
    /// until the next key, and passes the rest of `flow` on.
    pub(super) fn say(&mut self, flow: Flow) -> Flow {
        match flow {
            Flow::Say(m) => {
                self.status = Some(m);
                Flow::Continue
            }
            flow => flow,
        }
    }

    /// The reader's error, else the first a line read hit.
    fn read_error(&self) -> Option<ReadError> {
        let buf = self.views.text().buf();
        match buf.finished() {
            Some(Err(e)) => Some(e),
            _ => buf.read_error(),
        }
    }

    /// Draws the row on the bottom row, which needs two rows to exist,
    /// unless the line is hidden: in reverse video, or plain
    /// while a prompt is open, so the terminal's cursor shows on it,
    /// where on a reverse row it is the row's own color. "h help",
    /// naming whatever key shows help, is at the right edge unless help
    /// is unbound or up, a prompt is open, or it would come within two
    /// spaces of the text. The : prompt's completions follow it, the one
    /// filled in in the other video and on screen; the & prompt has the
    /// lines listed of the text's at the right edge, as the hint is. A
    /// prompt wider than the screen scrolls to keep the cursor on it, at
    /// the right edge. Returns the cell of an open prompt's cursor.
    pub(super) fn draw_status(&self, g: &mut Grid, at: At) -> Option<(usize, usize)> {
        let (w, h) = (g.width(), g.height());
        let row = self.row();
        if h < 2 || matches!(row, Row::Hidden) {
            return None;
        }
        let prompt = match row {
            Row::Prompt(p) => Some(p),
            _ => None,
        };
        let (st, mark) = if prompt.is_some() {
            (Style::default(), Style::default().reverse())
        } else {
            (Style::default().reverse(), Style::default())
        };
        g.fill_row(h - 1, st);
        let text: Vec<char> = self.status_text(&row, w, at).chars().collect();
        let xs = Layout::default().cells(&text);
        let cur = prompt.map(|p| xs[p.cursor()]);
        // At no width the cursor is off screen anyway; the offset must
        // still land on the line.
        let off = cur.map_or(0, |c| (c + 1).saturating_sub(w.max(1)));
        // The first glyph starting on screen: a wide one cut by the
        // edge leaves its cell blank.
        let first = (0..=text.len())
            .find(|&i| xs[i] >= off && layout::glyph_start(&xs, i))
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
        if self.focus == Focus::Prompting(Kind::Filter)
            && let Some(f) = &self.filter
        {
            let total = self.views.text().buf().len();
            let count: Vec<char> = format!("{}/{total}", f.listed()).chars().collect();
            if width + 2 + count.len() <= w {
                runes(g, w - count.len(), h - 1, &count, st);
            }
        }
        let help = Command {
            action: Action::Help,
            extend: false,
            arg: None,
        };
        if let Some(k) = self.keys.find(&help) {
            let hint: Vec<char> = format!("{k} help").chars().collect();
            if self.focus == Focus::Viewing
                && self.views.role() != Role::Help
                && width + 2 + hint.len() <= w
            {
                runes(g, w - hint.len(), h - 1, &hint, st);
            }
        }
        cur
    }

    /// The line the status line names: the cursor's, or the top row's
    /// while the cursor is scrolled off screen, so it says where the
    /// view is.
    pub(super) fn status_at(&mut self) -> At {
        if self.views.shown.cursor_visible() {
            At {
                line: self.views.shown.cursor().line,
                past: self.views.shown.at_end_row(),
            }
        } else {
            let top = self.views.shown.top();
            At {
                line: top.line,
                past: self.views.shown.is_end_row(top),
            }
        }
    }

    fn status_text(&self, row: &Row, width: usize, at: At) -> String {
        match row {
            Row::Prompt(p) => p.line(),
            Row::HelpHint => "help  motion scrolls, / searches, any other key returns".into(),
            Row::Message(s) => s.to_string(),
            Row::ReadError(e) => format!("read error: {e}"),
            Row::Summary => self.summary(width, at),
            Row::Hidden => String::new(),
        }
    }

    /// The input's name, which input of several, the line the cursor
    /// is on, the wrap mode and view style, and what is still being
    /// read. In a filter's list: the query, which listed line of how
    /// many, and the text's line it is, named with its input when there
    /// are several.
    fn summary(&self, width: usize, at: At) -> String {
        let tail = self.mode_and_reading();
        let buf = self.views.text().buf();
        // The end row is one past the last line, and no line.
        let past = if at.past { "+1" } else { "" };
        let at = at.line;
        if self.listing()
            && let Some(f) = &self.filter
        {
            let Some(li) = f.line(at) else {
                return format!("&{}  0/0  {tail}", f.query());
            };
            let (_, lines) = buf.part(li);
            let mut name = String::new();
            if self.names.len() > 1 {
                name = format!("{} ", self.input_name(li));
            }
            return format!(
                "&{}  {}{past}/{}  {name}line {}/{}  {tail}",
                f.query(),
                at + 1,
                f.listed(),
                li - lines.start + 1,
                lines.len()
            );
        }
        let (k, lines) = buf.part(at);
        let mut rest = String::new();
        if self.names.len() > 1 {
            rest = format!(" {}/{}", k + 1, self.names.len());
        }
        rest.push_str(&format!(
            "  line {}{past}/{}  {tail}",
            at - lines.start + 1,
            lines.len()
        ));
        let name = fit(
            &self.input_name(at),
            width.saturating_sub(rest.chars().count()),
        );
        format!("{name}{rest}")
    }

    /// The wrap mode and view style, and what is still being read.
    fn mode_and_reading(&self) -> String {
        let mut mode = match (self.settings.mode, self.settings.wrap_style) {
            (WrapMode::NoWrap, _) => "nowrap",
            (_, WrapStyle::Word) => "word wrap",
            _ => "wrap",
        }
        .to_string();
        if self.settings.view_style != ViewStyle::Color {
            mode = format!("{mode}  {}", self.settings.view_style);
        }
        if self.views.text().buf().finished().is_none() {
            mode.push_str("  reading…");
        }
        if self.next.is_some() {
            mode.push_str("  reloading…");
        }
        mode
    }

    /// What the status line calls the input line `at` came from: its
    /// name, or for stdin the window title in effect at that
    /// line, as a shell prompt sets to the directory.
    fn input_name(&self, at: usize) -> String {
        let buf = self.views.text().buf();
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
