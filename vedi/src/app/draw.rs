//! One frame: the text rows from the top, with the selection, control
//! characters, separators and markers, and the cursor.

use crate::app::text_view::{Row, Selection};
use crate::app::views::Role;
use crate::app::{App, Focus, Kind, STDIN};
use crate::text::ansi::{Link, Run, ViewStyle};
use crate::view::layout::{self, Layout, Mode};
use crate::view::screen::{Grid, Region, Terminal};
use crate::view::style::{Element, Style};

impl<T: Terminal> App<T> {
    /// Renders the text from `top`, the selection and search matches in
    /// the theme's styles, and the status line. While a `StartScreen`
    /// waits for EOF only the status line is drawn: the view is not
    /// known until then. While help is up the bindings are the text, and
    /// no cursor is shown; nor is one scrolled off the side. An open
    /// prompt has the cursor; at the & prompt the list's pointed line
    /// is in reverse video instead, once there is a query or the pointer
    /// has moved.
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
        if !self.views.shown.start_pending() {
            cur = self.draw_text(&mut g);
            self.draw_hints(&mut g);
        }
        // A prompt has the cursor.
        if !self.views.shown.cursor_shown() || self.focus != Focus::Viewing {
            cur = None;
        }
        let at = self.views.shown.at();
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
    /// A row below the text starts with a ~, as in less, unless end_markers
    /// is off. Every row carries the wrap_width bar when there is one,
    /// and a line's first row its number when they are on.
    fn draw_text(&mut self, g: &mut Grid) -> Option<(usize, usize)> {
        let rows = self.views.shown.rows();
        let geom = self.views.shown.geometry();
        let area = geom.text_area();
        let guide = geom.guide_col().map(|x| area.grid_col(x));
        let guide_st = self.settings.theme[Element::Guide];
        let mut p = self.views.shown.top();
        let mut cur = None;
        let mut y = 0;
        let mut matches = Vec::new();
        let mut match_line = None;
        let sel = self.views.shown.selection();
        while y < rows && p.line < self.views.shown.buf().len() {
            if self.views.shown.highlight() && match_line != Some(p.line) {
                self.views.shown.buf().text(p.line, &mut self.scratch);
                matches = self.views.shown.matcher().all(&self.scratch);
                match_line = Some(p.line);
            }
            if let Some(x) = self.draw_row(g, y, p, &matches, sel) {
                cur = Some((area.grid_col(x), y));
            }
            if p.col == Some(0) {
                self.draw_number(g, y, p.line);
            }
            if let Some(x) = guide {
                g.put(x, y, "│", guide_st, None);
            }
            let next = self.views.shown.next_row(p);
            if next == p {
                y += 1;
                break;
            }
            p = next;
            y += 1;
        }
        for y in y..rows {
            if geom.end_markers {
                area.put(g, 0, y, "~", guide_st, None);
            }
            if let Some(x) = guide {
                g.put(x, y, "│", guide_st, None);
            }
        }
        cur
    }

    /// Draws the row starting at `p` on screen row `y`, with `matches`
    /// the line's search matches and `sel` the selection, and returns
    /// the cursor's column of the text when the view shows it on this
    /// row.
    fn draw_row(
        &mut self,
        g: &mut Grid,
        y: usize,
        p: Row,
        matches: &[usize],
        sel: Option<Selection>,
    ) -> Option<usize> {
        let Some(col) = p.col else {
            self.draw_separator(g, y, p.line);
            return None;
        };
        // The end row: nothing but the cursor.
        if self.views.shown.is_end_row(p) {
            let cell = self.views.shown.cursor_on(p);
            return cell.filter(|c| c.shown).map(|c| c.x as usize);
        }
        let geom = self.views.shown.geometry();
        let w = geom.cols;
        let area = geom.text_area();
        let (cursor, xoff) = (self.views.shown.cursor(), self.views.shown.xoff());
        let pointed = self.focus == Focus::Prompting(Kind::Filter)
            && self.filter.as_ref().is_some_and(|f| f.pointer_shown())
            && cursor.line == p.line;
        let cell = self.views.shown.cursor_on(p);
        let ln = self.views.shown.line_layout(p.line);
        let line = self.views.shown.buf().line(p.line);
        let xs = ln.cells();
        let segi = ln.segment_at(col);
        let seg = ln.segments()[segi];
        let x0 = (xs[seg.start] + xoff) as isize;
        let n = line.text.len();
        let sel = sel.map_or(0..0, |s| s.cols(p.line, n));
        // The cursor's glyph [ci, cj), and the chars [cur_lo, cur_hi) of
        // the matches over it: matches may overlap, and those that do
        // are one.
        let m = self.views.shown.matcher().len();
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
        let theme = &self.settings.theme;
        let mut ri = 0;
        let mut i = seg.start;
        while i < end {
            let x = xs[i] as isize - x0;
            let (_, j) = ln.glyph(i);
            let selected = sel.contains(&i);
            if i == n {
                if selected {
                    area.put(g, x, y, " ", theme[Element::Selection], None);
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
                theme[Element::Selection]
            } else if i < cur_hi && cur_lo < j {
                theme[Element::CurrentMatch]
            } else if mi < matches.len() && matches[mi] < j {
                theme[Element::Match]
            } else {
                let st = if colored {
                    style_at(&line.runs, ri, i)
                } else {
                    Style::default()
                };
                if is_control(line.text[i]) {
                    theme[Element::Control].over(st)
                } else if pointed {
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
            glyph(g, area, x, y, &text, xs[i + 1] - xs[i], st, link);
            i = j;
        }
        // With edge markers on, text off either side of the screen is
        // marked at that edge. A wide char half under the > is dropped,
        // its cell kept in its style: the terminal would draw it over
        // the marker.
        let width = xs[n];
        if geom.edge_markers && geom.mode == Mode::NoWrap {
            if xoff > 0 && width > 0 {
                area.put(g, 0, y, "<", theme[Element::EdgeMarker], None);
            }
            if width > xoff + w && w > 0 {
                if w >= 2 {
                    let c = g.get(area.grid_col(w - 2), y);
                    if c.width == 2 {
                        let (st, link) = (c.style, c.link.clone());
                        area.put(g, w as isize - 2, y, " ", st, link);
                    }
                }
                let st = theme[Element::EdgeMarker];
                area.put(g, w as isize - 1, y, ">", st, None);
            }
        }
        // In wrap mode a row whose line goes on below ends in a marker,
        // in the column the layout leaves free.
        if geom.wrap_marks() && seg.end < n {
            let x = area.width() as isize - 1;
            area.put(g, x, y, "\\", theme[Element::EdgeMarker], None);
        }
        cell.filter(|c| c.shown).map(|c| c.x as usize)
    }

    /// Draws line `i`'s number at line_numbers_side's edge, counted
    /// in its input as the status line counts, right-aligned; in a
    /// filter's list, the number of the text's line it is.
    fn draw_number(&self, g: &mut Grid, y: usize, i: usize) {
        let geom = self.views.shown.geometry();
        let Some(x) = geom.numbers_col() else {
            return;
        };
        let line = match &self.filter {
            Some(f) if self.views.role() == Role::List => f.line(i),
            _ => Some(i),
        };
        let Some(line) = line else {
            return;
        };
        let (_, lines) = self.views.text().buf().part(line);
        let n = (line - lines.start + 1).to_string();
        let x = x + (geom.gutter - 1).saturating_sub(n.len());
        let st = self.settings.theme[Element::LineNumber];
        for (k, c) in n.chars().enumerate() {
            g.put(x + k, y, c.encode_utf8(&mut [0; 4]), st, None);
        }
    }

    /// Draws the row naming line `i`'s input: two dashes, the name as
    /// given, and dashes to the right edge, or wrap_width's, which cuts a
    /// wider name.
    fn draw_separator(&self, g: &mut Grid, y: usize, i: usize) {
        let area = self.views.shown.geometry().text_area();
        let name: Vec<char> = self
            .names
            .get(self.views.shown.buf().part_at(i))
            .map_or(STDIN, String::as_str)
            .chars()
            .collect();
        let (rule, name_st) = (
            self.settings.theme[Element::Separator],
            self.settings.theme[Element::SeparatorName],
        );
        let mut x = runes(g, area, 0, y, &['━', '━', ' '], rule);
        x += runes(g, area, x, y, &name, name_st);
        area.put(g, x as isize, y, " ", rule, None);
        for x in x + 1..area.width() {
            area.put(g, x as isize, y, "━", rule, None);
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

/// Draws the cluster `text` at (`x`, `y`) of `area` in `width` cells:
/// a tab as that many spaces, a C0 control as ^X (DEL as ^?), the X
/// only given a second cell, a C1 control as U+FFFD, anything else as
/// itself with its marks. A tab or ^X cut by the area's right edge
/// keeps the cells short of it.
#[allow(clippy::too_many_arguments)]
fn glyph(
    g: &mut Grid,
    area: Region,
    x: isize,
    y: usize,
    text: &str,
    width: usize,
    st: Style,
    link: Option<Link>,
) {
    let c = text.chars().next().unwrap_or(' ');
    match c {
        // Only the cells in the area: a tab may be thousands wide.
        '\t' => {
            let end = (width as isize).min(area.width() as isize - x);
            for k in (-x).max(0)..end {
                area.put(g, x + k, y, " ", st, link.clone());
            }
        }
        '\x7f' => {
            area.put(g, x, y, "^", st, link.clone());
            if width > 1 {
                area.put(g, x + 1, y, "?", st, link);
            }
        }
        c if c < ' ' => {
            area.put(g, x, y, "^", st, link.clone());
            let caret = char::from(c as u8 + 0x40);
            if width > 1 {
                area.put(g, x + 1, y, caret.encode_utf8(&mut [0; 4]), st, link);
            }
        }
        // A name or title may carry C1, which some terminals take for
        // escapes; content drops it on the way in.
        c if c.is_control() => {
            area.put(g, x, y, "\u{fffd}", st, link);
        }
        _ => {
            area.put(g, x, y, text, st, link);
        }
    }
}

/// Draws `text` from (`x`, `y`) of `area` glyph by glyph and returns its
/// width in cells.
pub(super) fn runes(
    g: &mut Grid,
    area: Region,
    x: usize,
    y: usize,
    text: &[char],
    st: Style,
) -> usize {
    let xs = Layout::default().cells(text);
    let mut i = 0;
    while i < text.len() {
        let (_, j) = layout::glyph(&xs, i);
        let s: String = text[i..j].iter().collect();
        let cell = (x + xs[i]) as isize;
        glyph(g, area, cell, y, &s, xs[i + 1] - xs[i], st, None);
        i = j;
    }
    xs[text.len()]
}
