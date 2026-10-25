//! One frame: the text rows from the top, with the selection, control
//! characters, separators and markers, and the cursor.

use crate::app::text_view::Row;
use crate::app::{App, Focus, Kind, STDIN};
use crate::text::ansi::{Link, Run, ViewStyle};
use crate::text::buffer::Pos;
use crate::view::layout::{self, Layout, Mode};
use crate::view::screen::{Grid, Terminal};
use crate::view::style::{CUR_MATCH, EDGE, MATCH, SELECTION, SEP_NAME, SEP_RULE, Style};

impl<T: Terminal> App<T> {
    /// Renders the text from `top`, the selection in reverse, search
    /// matches in `MATCH`, and the status line. While a `StartScreen`
    /// waits for EOF only the status line is drawn: the view is not
    /// known until then. While help is up the bindings are the text, and
    /// no cursor is shown; nor is one scrolled off the side. An open
    /// prompt has the cursor; at the & prompt the list's pointed line
    /// is in reverse video instead.
    pub fn draw(&mut self) {
        self.sync_view();
        let (w, h) = self.screen.size();
        let mut g = std::mem::replace(&mut self.frame, Grid::new(0, 0));
        if (g.width(), g.height()) == (w, h) {
            g.clear();
        } else {
            g = Grid::new(w, h);
        }
        let mut cur = None;
        if !self.view.start_pending() {
            cur = self.draw_text(&mut g);
        }
        // A prompt has the cursor.
        if !self.view.cursor_shown() || self.focus != Focus::Viewing {
            cur = None;
        }
        let at = self.status_at();
        let cur = self.draw_status(&mut g, at).or(cur);
        let cur = cur.filter(|&(x, _)| x < w);
        self.screen.present(&g, cur);
        self.frame = g;
        if self.pending {
            self.last_draw = Some((self.clock)());
        }
        self.pending = false;
    }

    /// Draws the visible rows and returns the cursor's cell, if shown.
    fn draw_text(&mut self, g: &mut Grid) -> Option<(usize, usize)> {
        let rows = self.view.rows();
        let mut p = self.view.top();
        let mut cur = None;
        let mut y = 0;
        let mut matches = Vec::new();
        let mut match_line = None;
        while y < rows && p.line < self.view.buf().len() {
            if self.view.highlight() && match_line != Some(p.line) {
                self.view.buf().text(p.line, &mut self.scratch);
                matches = self.view.matcher().all(&self.scratch);
                match_line = Some(p.line);
            }
            if let Some(x) = self.draw_row(g, y, p, &matches) {
                cur = Some((x, y));
            }
            let next = self.view.next_row(p);
            if next == p {
                break;
            }
            p = next;
            y += 1;
        }
        cur
    }

    /// Draws the row starting at `p` on screen row `y`, with `matches`
    /// the line's search matches, and returns the cursor's column when
    /// the view shows it on this row.
    fn draw_row(&mut self, g: &mut Grid, y: usize, p: Row, matches: &[usize]) -> Option<usize> {
        let Some(col) = p.col else {
            self.draw_separator(g, y, p.line);
            return None;
        };
        let w = g.width();
        let geom = self.view.geometry();
        let (cursor, xoff) = (self.view.cursor(), self.view.xoff());
        let pointed = self.focus == Focus::Prompting(Kind::Filter) && cursor.line == p.line;
        let cell = self.view.cursor_on(p);
        let ln = self.view.line_layout(p.line);
        let line = self.view.buf().line(p.line);
        let xs = ln.cells();
        let segi = ln.segment_at(col);
        let seg = ln.segments()[segi];
        let x0 = (xs[seg.start] + xoff) as isize;
        let sel = self.view.selection();
        let n = line.text.len();
        // The cursor's glyph [ci, cj), and the chars [cur_lo, cur_hi) of
        // the matches over it: matches may overlap, and those that do
        // are one.
        let m = self.view.matcher().len();
        let (mut cur_lo, mut cur_hi) = (0, 0);
        if cursor.line == p.line {
            let (ci, cj) = ln.glyph(cursor.col);
            // Searched, not scanned: a long line is drawn a row at a
            // time, each with all its matches.
            let from = (ci + 1).saturating_sub(m);
            for &k in &matches[matches.partition_point(|&k| k < from)..] {
                if k >= cj {
                    break;
                }
                if cur_hi == 0 {
                    cur_lo = k;
                }
                cur_hi = k + m;
            }
        }
        let mut mi = 0;
        // The newline is a virtual cell after the last char, on the last
        // row: the cursor can rest on it, and it shows as one reverse
        // cell when selected.
        let end = seg.end + usize::from(segi + 1 == ln.rows());
        let colored = self.settings.view_style == ViewStyle::Color;
        let mut ri = 0;
        let mut i = seg.start;
        while i < end {
            let x = xs[i] as isize - x0;
            let (_, j) = ln.glyph(i);
            let pos = Pos {
                line: p.line,
                col: i,
            };
            let selected = sel.is_some_and(|(s, e)| s <= pos && pos < e);
            if i == n {
                if selected {
                    put(g, x, y, " ", SELECTION, None);
                }
                break;
            }
            // Runs and matches are sorted: skip those ending before i.
            while ri < line.runs.len() && line.runs[ri].end <= i {
                ri += 1;
            }
            while mi < matches.len() && matches[mi] + m <= i {
                mi += 1;
            }
            let st = if selected {
                SELECTION
            } else if i < cur_hi && cur_lo < j {
                CUR_MATCH
            } else if mi < matches.len() && matches[mi] < j {
                MATCH
            } else {
                let st = if colored {
                    style_at(&line.runs, ri, i)
                } else {
                    Style::default()
                };
                if pointed || is_control(line.text[i]) {
                    st.reverse()
                } else {
                    st
                }
            };
            let link = match line.runs.get(ri) {
                Some(r) if r.start <= i => r.link.clone(),
                _ => None,
            };
            let text: String = line.text[i..j].iter().collect();
            glyph(g, x, y, &text, xs[i + 1] - xs[i], st, link);
            i = j;
        }
        // With edge markers on, text off either side of the screen is
        // marked at that edge. A wide char half under the > is dropped,
        // its cell kept in its style: the terminal would draw it over
        // the marker.
        let width = xs[n];
        if geom.edge_markers && geom.mode == Mode::NoWrap {
            if xoff > 0 && width > 0 {
                put(g, 0, y, "<", EDGE, None);
            }
            if width > xoff + w && w > 0 {
                if w >= 2 {
                    let c = g.get(w - 2, y);
                    if c.width == 2 {
                        let (st, link) = (c.style, c.link.clone());
                        g.put(w - 2, y, " ", st, link);
                    }
                }
                put(g, w as isize - 1, y, ">", EDGE, None);
            }
        }
        // In wrap mode a row whose line goes on below ends in a marker,
        // in the column the layout leaves free.
        if geom.wrap_marks() && seg.end < n {
            put(g, w as isize - 1, y, "\\", EDGE, None);
        }
        cell.filter(|c| c.shown).map(|c| c.x as usize)
    }

    /// Draws the row naming line `i`'s input: two dashes, the name as
    /// given, and dashes to the right edge, which cuts a wider name.
    fn draw_separator(&self, g: &mut Grid, y: usize, i: usize) {
        let name: Vec<char> = self
            .names
            .get(self.view.buf().part_at(i))
            .map_or(STDIN, String::as_str)
            .chars()
            .collect();
        let mut x = runes(g, 0, y, &['━', '━', ' '], SEP_RULE);
        x += runes(g, x, y, &name, SEP_NAME);
        g.put(x, y, " ", SEP_RULE, None);
        x += 1;
        while x < g.width() {
            g.put(x, y, "━", SEP_RULE, None);
            x += 1;
        }
    }
}

/// The style of char `i`, where `runs[ri]` is the first run not ending
/// before it.
fn style_at(runs: &[Run], ri: usize, i: usize) -> Style {
    match runs.get(ri) {
        Some(r) if r.start <= i => r.style,
        _ => Style::default(),
    }
}

/// Whether `c` is drawn as ^X: a C0 control other than tab, or DEL.
fn is_control(c: char) -> bool {
    c < ' ' && c != '\t' || c == '\x7f'
}

/// Draws the cluster `s` at (`x`, `y`) if that is on screen.
fn put(g: &mut Grid, x: isize, y: usize, s: &str, st: Style, link: Option<Link>) {
    if let Ok(x) = usize::try_from(x) {
        g.put(x, y, s, st, link);
    }
}

/// Draws the cluster `text` at (`x`, `y`): a tab as `width` spaces, a C0
/// control as ^X (DEL as ^?), a C1 control as U+FFFD, anything else as
/// itself with its marks.
fn glyph(
    g: &mut Grid,
    x: isize,
    y: usize,
    text: &str,
    width: usize,
    st: Style,
    link: Option<Link>,
) {
    let c = text.chars().next().unwrap_or(' ');
    match c {
        '\t' => {
            for k in 0..width as isize {
                put(g, x + k, y, " ", st, link.clone());
            }
        }
        '\x7f' => {
            put(g, x, y, "^", st, link.clone());
            put(g, x + 1, y, "?", st, link);
        }
        c if c < ' ' => {
            put(g, x, y, "^", st, link.clone());
            let caret = char::from(c as u8 + 0x40);
            put(g, x + 1, y, caret.encode_utf8(&mut [0; 4]), st, link);
        }
        // A name or title may carry C1, which some terminals take for
        // escapes; content drops it on the way in.
        c if c.is_control() => put(g, x, y, "\u{fffd}", st, link),
        _ => put(g, x, y, text, st, link),
    }
}

/// Draws `text` from (`x`, `y`) glyph by glyph and returns its width in
/// cells.
pub(super) fn runes(g: &mut Grid, x: usize, y: usize, text: &[char], st: Style) -> usize {
    let xs = Layout::default().cells(text);
    let mut i = 0;
    while i < text.len() {
        let (_, j) = layout::glyph(&xs, i);
        let s: String = text[i..j].iter().collect();
        glyph(g, (x + xs[i]) as isize, y, &s, xs[i + 1] - xs[i], st, None);
        i = j;
    }
    xs[text.len()]
}
