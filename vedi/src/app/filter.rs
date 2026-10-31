//! The & filter: the lines that match a query, listed in the text's
//! place as a text view of their own, so they scroll, select and copy
//! as the text does; Enter goes to one.

use std::mem;

use crate::app::prompt::Edit;
use crate::app::text_view::{Motion, Selection, Start, TextView};
use crate::app::views::Role;
use crate::app::{App, Flow, Focus, Kind};
use crate::input::action::{Action, Command};
use crate::input::key::{Key, KeyCode, Mods};
use crate::text::ansi::ViewStyle;
use crate::text::buffer::{Buffer, Pos, Progress, Storage};
use crate::text::index::{BLOCK_LINES, Indexer};
use crate::text::search::{self, Matcher};
use crate::view::render::{osc8, sgr};
use crate::view::screen::Terminal;

pub(super) struct Filter {
    state: State,
    /// The list, after Enter went to a line in the text.
    parked: Option<TextView>,
}

/// The query and what it found.
struct State {
    matcher: Matcher,
    /// The text's line each listed line is.
    lines: Vec<usize>,
    /// How many of the text's lines were looked at: those after came
    /// since, and are listed as they come.
    scanned: usize,
    /// Indexes the list as it is fed.
    ix: Indexer,
    /// The last line fed, and whether as its bytes came.
    last: Option<(usize, bool)>,
}

/// The text's bytes a block at a time, for copying lines undecoded.
struct Blocks<'a> {
    buf: &'a Buffer,
    k: Option<usize>,
    bytes: Vec<u8>,
    /// Where each line of the block starts, and the end of the last.
    starts: Vec<usize>,
}

impl<'a> Blocks<'a> {
    fn new(buf: &'a Buffer) -> Blocks<'a> {
        Blocks {
            buf,
            k: None,
            bytes: Vec::new(),
            starts: Vec::new(),
        }
    }

    /// Line `li`'s bytes, its `\n` included, or `None` when they cannot
    /// be read.
    fn line(&mut self, li: usize) -> Option<&[u8]> {
        let k = li / BLOCK_LINES;
        if self.k != Some(k) {
            // A read that fails leaves nothing of the block before.
            self.k = None;
            self.bytes.clear();
            self.starts.clear();
            let mut scratch = Vec::new();
            self.bytes = self.buf.raw_block(k, &mut scratch).ok()?.to_vec();
            self.k = Some(k);
            self.starts.push(0);
            let mut at = 0;
            while at < self.bytes.len() {
                at += self.bytes[at..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(self.bytes.len() - at, |i| i + 1);
                self.starts.push(at);
            }
        }
        let j = li - k * BLOCK_LINES;
        let (s, e) = (*self.starts.get(j)?, *self.starts.get(j + 1)?);
        Some(&self.bytes[s..e])
    }
}

impl Filter {
    pub(super) fn query(&self) -> &str {
        self.state.matcher.pattern()
    }

    /// How many lines are listed.
    pub(super) fn listed(&self) -> usize {
        self.state.lines.len()
    }

    /// The text's line listed line `i` is.
    pub(super) fn line(&self, i: usize) -> Option<usize> {
        self.state.lines.get(i).copied()
    }
}

impl State {
    /// Lists the lines of `text` after `scanned` that match, as
    /// progress for the list, which ends when the text does. `done` is
    /// the list's: progress after its end would be dropped.
    fn extend(
        &mut self,
        text: &Buffer,
        style: ViewStyle,
        done: bool,
        apply: &mut impl FnMut(Progress),
    ) {
        let n = text.len();
        let mut found = Vec::new();
        if self.matcher.is_empty() {
            found.extend(self.scanned..n);
        } else {
            search::lines(text, &self.matcher, self.scanned..n, &mut found);
        }
        self.scanned = n;
        self.feed(text, style, &found, apply);
        self.lines.extend(found);
        if let Some(end) = text.finished()
            && !done
        {
            apply(Progress::Data {
                chunk: None,
                indexed: self.ix.finish(),
            });
            // The text's last line, unterminated, is listed as it is.
            let cut = !text.trailing_newline() && self.lines.last() == n.checked_sub(1).as_ref();
            apply(Progress::Done {
                err: end.err(),
                trailing_newline: !cut,
            });
        }
    }

    /// Appends `lines` of `text` to the list. A line goes as its bytes
    /// came when the list's state is the text's before it: it is the
    /// first, it follows a line that went so, or it starts a block,
    /// whose state the index keeps and is written first. Any other is
    /// written as the view shows it, the style it carries in included,
    /// after a reset for what a raw line left. Raw, every line is its
    /// bytes. A line without a newline in the text, its last after EOF,
    /// gets none here either.
    fn feed(
        &mut self,
        text: &Buffer,
        style: ViewStyle,
        lines: &[usize],
        apply: &mut impl FnMut(Progress),
    ) {
        if lines.is_empty() {
            return;
        }
        let mut out = Vec::new();
        let mut blocks = Blocks::new(text);
        let literal = style == ViewStyle::Raw;
        for &li in lines {
            let known = li == 0 || self.last.is_some_and(|(l, raw)| raw && l + 1 == li);
            let mut resync = None;
            if !known && !literal && li % BLOCK_LINES == 0 {
                resync = text.block_state(li / BLOCK_LINES);
            }
            let raw = (known || literal || resync.is_some())
                && blocks.line(li).is_some_and(|b| {
                    if let Some((st, link)) = &resync {
                        out.extend_from_slice(b"\x1b]8;;\x1b\\");
                        sgr(&mut out, *st);
                        osc8(&mut out, None, link.as_ref());
                    }
                    out.extend_from_slice(b);
                    true
                });
            if !raw {
                if self.last.is_some_and(|(_, raw)| raw) && !literal {
                    out.extend_from_slice(b"\x1b]8;;\x1b\\\x1b[0m");
                }
                let n = text.line(li).text.len();
                let line = Selection {
                    start: Pos { line: li, col: 0 },
                    end: Pos { line: li, col: n },
                    newline: false,
                };
                // A line that cannot be read is empty on screen, and here.
                line.shown(text, style, &mut out);
                // The bytes say whether the line ended; unread, the last
                // line's ending is the input's, known once it is done.
                let ended = blocks.line(li).map_or_else(
                    || li + 1 < text.len() || text.finished().is_none() || text.trailing_newline(),
                    |b| b.ends_with(b"\n"),
                );
                if ended {
                    // A \r the text ends with would be taken with the
                    // \n, as one is from the input.
                    if out.last() == Some(&b'\r') {
                        out.push(b'\r');
                    }
                    out.push(b'\n');
                }
            }
            self.last = Some((li, raw));
        }
        let indexed = self.ix.feed(&out);
        apply(Progress::Data {
            chunk: Some(out.into()),
            indexed,
        });
    }

    /// Starts the list over: `lines`, the matches among the first
    /// `scanned` lines of the text, then the lines since. The list
    /// keeps its place, as a view does over a reload.
    fn relist(
        &mut self,
        text: &Buffer,
        list: &mut TextView,
        style: ViewStyle,
        lines: Vec<usize>,
        scanned: usize,
    ) {
        self.lines.clear();
        self.scanned = scanned;
        self.ix = Indexer::new();
        self.last = None;
        let mut buf = Buffer::new(Storage::Memory(Vec::new()), style == ViewStyle::Raw);
        self.feed(text, style, &lines, &mut |p| buf.apply(p));
        self.lines = lines;
        self.extend(text, style, false, &mut |p| buf.apply(p));
        list.change_text(|b| *b = buf);
        list.set_matcher(self.matcher.clone());
    }

    /// Puts the cursor on listed line `line`, at its first match.
    fn point(&self, list: &mut TextView, line: usize, scratch: &mut Vec<char>) {
        let n = list.buf().len();
        if n == 0 {
            return;
        }
        let line = line.min(n - 1);
        list.buf().text(line, scratch);
        let col = self.matcher.find(scratch, 0).unwrap_or(0);
        list.go(Motion::To(Pos { line, col }), false);
    }
}

impl<T: Terminal> App<T> {
    /// Whether the list is up: shown, or under help.
    pub(super) fn listing(&self) -> bool {
        self.views.holds(Role::List)
    }

    /// The text's buffer, the list's view and the filter's state.
    fn filter_parts(&mut self) -> Option<(&Buffer, &mut TextView, &mut State)> {
        let f = self.filter.as_mut()?;
        Some(match &mut f.parked {
            Some(list) => (self.views.text().buf(), list, &mut f.state),
            None => {
                let (text, list) = self.views.text_and(Role::List);
                (text.buf(), list, &mut f.state)
            }
        })
    }

    /// Opens the & prompt over the list: the one Enter left, with its
    /// query and its place, or a new one, over every line.
    pub(super) fn open_filter(&mut self) {
        self.end_drag();
        match &mut self.filter {
            None => {
                let literal = self.settings.view_style == ViewStyle::Raw;
                let buf = Buffer::new(Storage::Memory(Vec::new()), literal);
                let list = TextView::new(buf, Start::default(), self.geometry());
                self.views.raise(Role::List, list);
                self.filter = Some(Filter {
                    state: State {
                        matcher: Matcher::new(""),
                        lines: Vec::new(),
                        scanned: 0,
                        ix: Indexer::new(),
                        last: None,
                    },
                    parked: None,
                });
                self.relist(Vec::new(), 0);
                self.point(0);
            }
            Some(f) => {
                if let Some(list) = f.parked.take() {
                    self.views.raise(Role::List, list);
                }
            }
        }
        let query = self.filter.as_ref().map_or("", Filter::query).to_string();
        self.query.open_with('&', &query);
        self.focus(Focus::Prompting(Kind::Filter));
    }

    /// Leaves the filter: the text as it was, the query forgotten.
    pub(super) fn close_filter(&mut self) {
        if self.views.role() == Role::List {
            self.views.lower(Role::List);
        }
        self.filter = None;
    }

    /// Enter on a listed line: the text, with the cursor at the line's
    /// place in it, in the middle of the screen, and the query
    /// highlighted as a search that found it there; n and N go on from
    /// it.
    fn jump(&mut self) {
        let Some(f) = &self.filter else {
            return;
        };
        let p = self.views.shown.cursor();
        let Some(line) = f.line(p.line) else {
            return;
        };
        let m = f.state.matcher.clone();
        let list = self.views.lower(Role::List);
        if let Some(f) = &mut self.filter {
            f.parked = Some(list);
        }
        // The screen may have changed while the list was up.
        self.sync_view();
        self.views.shown.land(m, Pos { line, col: p.col });
    }

    /// What a key does in the list that it does not in the text: Enter
    /// with no selection goes to the cursor's line, Esc with none
    /// leaves. `None` for any other key.
    pub(super) fn list_key(&mut self, c: &Command) -> Option<Flow> {
        if !self.listing() || c.extend || self.views.shown.has_selection() {
            return None;
        }
        match c.action {
            Action::CopyAndQuit => self.jump(),
            Action::ClearSelection => self.close_filter(),
            _ => return None,
        }
        Some(Flow::Continue)
    }

    /// A key at the & prompt. Ctrl+n, Ctrl+p, PgDn and PgUp move the
    /// pointer, by a line or a page; Enter closes the prompt and leaves
    /// the list up; Esc, Ctrl+g and Ctrl+c leave the filter; any other
    /// key edits the query, Up and Down among them, which recall earlier
    /// ones, and the list follows it.
    pub(super) fn filter_key(&mut self, key: Key) -> Flow {
        let rows = self.views.shown.rows() as isize;
        let k = key.normalized();
        let by = match (k.code, k.mods) {
            (KeyCode::Char('n'), Mods::CTRL) => Some(1),
            (KeyCode::Char('p'), Mods::CTRL) => Some(-1),
            (KeyCode::PageUp, Mods::NONE) => Some(-rows),
            (KeyCode::PageDown, Mods::NONE) => Some(rows),
            _ => None,
        };
        if let Some(by) = by {
            let line = self.views.shown.cursor().line.saturating_add_signed(by);
            self.point(line);
            return Flow::Continue;
        }
        match self.query.key(key) {
            Edit::Editing => self.query_changed(),
            Edit::Submit(_) => self.focus(Focus::Viewing),
            Edit::Cancel => {
                self.close_filter();
                self.focus(Focus::Viewing);
            }
        }
        Flow::Continue
    }

    /// Lists for the query as the prompt has it, if it changed, the
    /// pointer back at the top. A query that grew around the last one
    /// narrows the lines listed rather than search the text again.
    pub(super) fn query_changed(&mut self) {
        let Some(f) = &self.filter else {
            return;
        };
        let q = self.query.text();
        let old = f.state.matcher.clone();
        if q == old.pattern() {
            return;
        }
        let style = self.settings.view_style;
        let Some((text, list, state)) = self.filter_parts() else {
            return;
        };
        state.matcher = Matcher::new(&q);
        if !old.is_empty() && q.contains(old.pattern()) {
            let mut scratch = Vec::new();
            let kept: Vec<usize> = state
                .lines
                .iter()
                .copied()
                .filter(|&li| {
                    text.text(li, &mut scratch);
                    state.matcher.find(&scratch, 0).is_some()
                })
                .collect();
            let scanned = state.scanned;
            state.relist(text, list, style, kept, scanned);
        } else {
            state.relist(text, list, style, Vec::new(), 0);
        }
        self.point(0);
    }

    fn relist(&mut self, lines: Vec<usize>, scanned: usize) {
        let style = self.settings.view_style;
        if let Some((text, list, state)) = self.filter_parts() {
            state.relist(text, list, style, lines, scanned);
        }
    }

    /// Lists the text as it is now, the list keeping its place.
    pub(super) fn refilter(&mut self) {
        self.relist(Vec::new(), 0);
    }

    /// Lists the text's new lines.
    pub(super) fn extend_filter(&mut self) {
        let style = self.settings.view_style;
        if let Some((text, list, state)) = self.filter_parts() {
            let done = list.buf().finished().is_some();
            state.extend(text, style, done, &mut |p| list.apply(p));
        }
    }

    fn point(&mut self, line: usize) {
        let mut scratch = mem::take(&mut self.scratch);
        if let Some((_, list, state)) = self.filter_parts() {
            state.point(list, line, &mut scratch);
        }
        self.scratch = scratch;
    }
}
