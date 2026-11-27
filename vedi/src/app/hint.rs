//! Hints: a label on each search match shown, or each line with text
//! while no search is highlighted, typed to go there; in the filter's
//! list, to that place in the text, as Enter goes.

use std::iter;
use std::ops::Range;

use crate::app::text_view::{Motion, Shown};
use crate::app::views::Role;
use crate::app::{App, Flow};
use crate::input::key::{Key, KeyCode, Mods};
use crate::input::setting::HintOrder;
use crate::text::buffer::{Buffer, Pos};
use crate::text::search::Matcher;
use crate::view::layout::Segment;
use crate::view::screen::{Grid, Terminal};
use crate::view::style::Element;

/// What a label goes to, and the cell of the text it is drawn from.
#[derive(Copy, Clone)]
struct Target {
    y: usize,
    x: usize,
    pos: Pos,
    on_match: bool,
}

/// Digits first, so the top lines are 1 to 9.
const ALPHABET: &[u8] = b"123456789abcdefghijklmnopqrstuvwxyz";

/// `n` labels, none the start of another: one character each while
/// they last, then the alphabet's last characters lead two, as few as
/// cover the rest. Beyond what two cover, lines go unlabeled.
fn labels(n: usize) -> Vec<String> {
    let k = ALPHABET.len();
    let leads = if n <= k {
        0
    } else {
        (n - k).div_ceil(k - 1).min(k)
    };
    let mut out: Vec<String> = ALPHABET[..k - leads]
        .iter()
        .map(|&c| char::from(c).to_string())
        .collect();
    for &a in &ALPHABET[k - leads..] {
        for &b in ALPHABET {
            out.push(format!("{}{}", char::from(a), char::from(b)));
        }
    }
    out.truncate(n);
    out
}

/// The label of each of `n` targets, in screen order, `labels` given
/// from the end `order` names; `None` past the last.
fn ordered(n: usize, order: HintOrder) -> Vec<Option<String>> {
    let ls = labels(n);
    let none = iter::repeat_n(None, n - ls.len());
    match order {
        HintOrder::TopDown => ls.into_iter().map(Some).chain(none).collect(),
        HintOrder::BottomUp => none.chain(ls.into_iter().rev().map(Some)).collect(),
    }
}

/// Whether a line's `text`, from row `seg` on, draws more than spaces:
/// on that row within the cells `shown`, nowrap's scroll being one
/// way to leave none there, or on any row below. A control but tab is
/// drawn, as `^X` or U+FFFD.
fn shows_text(text: &[char], xs: &[usize], seg: Segment, shown: Range<usize>) -> bool {
    (seg.start..text.len()).any(|i| {
        let c = text[i];
        let blank = c == '\t' || c.is_whitespace() && !c.is_control();
        !blank && (i >= seg.end || shown.contains(&xs[i]))
    })
}

/// The targets whose labels fit, left to right, after the last label
/// kept on their row and before the edge at `right`. Fewer targets
/// never take longer labels, in either order, so those kept still fit
/// when labeled again; shorter ones may let more in, so they are
/// fitted again until none is dropped.
fn fit(mut targets: Vec<Target>, right: usize, order: HintOrder) -> Vec<Target> {
    loop {
        let ls = ordered(targets.len(), order);
        let n = targets.len();
        let mut end: Option<(usize, usize)> = None;
        let mut i = 0;
        targets.retain(|t| {
            let label = ls[i].as_ref().filter(|l| {
                let free = end.is_none_or(|(y, e)| y != t.y || t.x >= e);
                free && t.x + l.len() <= right
            });
            let keep = label.is_some();
            if let Some(l) = label {
                end = Some((t.y, t.x + l.len()));
            }
            i += 1;
            keep
        });
        if targets.len() == n {
            return targets;
        }
    }
}

/// The targets on `rows`, as many as fit: `m`'s matches within the
/// `right` cells shown, or with none, each line's first row that shows
/// text.
fn targets(
    rows: &[Shown],
    buf: &Buffer,
    m: Option<&Matcher>,
    right: usize,
    order: HintOrder,
) -> Vec<Target> {
    let (mut lines, mut matches) = (Vec::new(), Vec::new());
    let (mut text, mut found) = (Vec::new(), Vec::new());
    let mut prev = None;
    for (y, row) in rows.iter().enumerate() {
        let Shown::Text(r) = row else {
            continue;
        };
        let first = prev != Some(r.line);
        prev = Some(r.line);
        let (seg, xs) = (r.segment(), r.ln.cells());
        let shown = r.x0..r.x0 + right;
        if first {
            buf.text(r.line, &mut text);
            found = m.map_or_else(Vec::new, |m| m.all(&text));
            if shows_text(&text, xs, seg, shown.clone()) {
                let pos = Pos {
                    line: r.line,
                    col: seg.start,
                };
                lines.push(Target {
                    y,
                    x: 0,
                    pos,
                    on_match: false,
                });
            }
        }
        // A match is on the row its glyph starts on; matches are
        // sorted, so their glyphs are too.
        let lo = found.partition_point(|&k| k < seg.start);
        for &k in &found[lo..] {
            let (i, _) = r.ln.glyph(k);
            if i >= seg.end {
                break;
            }
            if i >= seg.start && shown.contains(&xs[i]) {
                let pos = Pos {
                    line: r.line,
                    col: k,
                };
                matches.push(Target {
                    y,
                    x: xs[i] - r.x0,
                    pos,
                    on_match: true,
                });
            }
        }
    }
    let targets = if matches.is_empty() { lines } else { matches };
    fit(targets, right, order)
}

impl<T: Terminal> App<T> {
    /// Labels the matches or lines shown, unless none is.
    pub(super) fn open_hints(&mut self) -> Flow {
        if self.hint_targets().is_empty() {
            return Flow::Fail("no lines to label".to_string());
        }
        self.end_drag();
        self.hint = Some(String::new());
        Flow::Continue
    }

    /// The highlighted search's matches shown, top down, left to
    /// right; with none, each line's first row shown.
    fn hint_targets(&mut self) -> Vec<Target> {
        let v = &mut self.views.shown;
        let mut walk = v.shown_rows();
        let rows: Vec<Shown> = iter::from_fn(|| walk.next(v)).collect();
        let right = v.geometry().wrap_cols();
        let m = (v.highlight() && !v.matcher().is_empty()).then(|| v.matcher());
        targets(&rows, v.buf(), m, right, self.settings.hint_order)
    }

    /// The labels left for what was typed: the part still to type, and
    /// where it goes.
    fn hints(&mut self) -> Vec<(String, Target)> {
        if self.hint.is_none() {
            return Vec::new();
        }
        let targets = self.hint_targets();
        let typed = self.hint.as_deref().unwrap_or_default();
        ordered(targets.len(), self.settings.hint_order)
            .into_iter()
            .zip(targets)
            .filter_map(|(l, t)| Some((l?.strip_prefix(typed)?.to_string(), t)))
            .collect()
    }

    /// A key while the labels are up: a label's character narrows them,
    /// and the last goes to its line. Any other key takes them down and
    /// does nothing else.
    pub(super) fn hint_key(&mut self, k: Key) -> Flow {
        let Some(mut typed) = self.hint.take() else {
            return Flow::Continue;
        };
        let k = k.normalized();
        let KeyCode::Char(c) = k.code else {
            return Flow::Continue;
        };
        if k.mods != Mods::NONE || !c.is_ascii() || !ALPHABET.contains(&(c as u8)) {
            return Flow::Continue;
        }
        typed.push(c);
        self.hint = Some(typed);
        let left = self.hints();
        match &left[..] {
            [(rest, t)] if rest.is_empty() => {
                self.hint = None;
                self.go_hint(*t);
            }
            [] => self.hint = None,
            _ => {}
        }
        Flow::Continue
    }

    /// Goes to `t`; a match counts as one a search landed on, so n and
    /// N step from it.
    fn go_hint(&mut self, t: Target) {
        self.views.shown.go(Motion::To(t.pos), false);
        if self.views.role() == Role::List {
            self.jump();
        } else if t.on_match {
            self.views.shown.recount();
        }
    }

    /// Draws the labels over the start of what they go to.
    pub(super) fn draw_hints(&mut self, g: &mut Grid) {
        let st = self.settings.theme[Element::Hint];
        let area = self.views.shown.geometry().text_area();
        for (label, t) in self.hints() {
            for (i, c) in label.chars().enumerate() {
                let x = (t.x + i) as isize;
                area.put(g, x, t.y, c.encode_utf8(&mut [0; 4]), st, None);
            }
        }
    }
}
