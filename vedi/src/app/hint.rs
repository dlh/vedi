//! Hints: a label on each search match shown, or each line with text
//! while no search is highlighted, typed to go there; in the filter's
//! list, to that place in the text, as Enter goes.

use std::iter;
use std::mem;
use std::ops::Range;

use crate::app::text_view::Motion;
use crate::app::views::Role;
use crate::app::{App, Flow};
use crate::input::key::{Key, KeyCode, Mods};
use crate::input::setting::HintOrder;
use crate::text::buffer::Pos;
use crate::view::layout::Segment;
use crate::view::screen::{Grid, Terminal};
use crate::view::style::Element;

/// What a label goes to, and the cell it is drawn from.
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
        let mut scratch = mem::take(&mut self.scratch);
        let v = &mut self.views.shown;
        let searched = v.highlight() && !v.matcher().is_empty();
        let right = v.geometry().wrap_cols();
        let (mut lines, mut matches) = (Vec::new(), Vec::new());
        let mut prev = None;
        let mut p = v.top();
        for y in 0..v.rows() {
            if p.line >= v.buf().len() {
                break;
            }
            if let Some(col) = p.col
                && !v.is_end_row(p)
            {
                let first = prev != Some(p.line);
                prev = Some(p.line);
                let ln = v.line_layout(p.line);
                let seg = ln.segments()[ln.segment_at(col)];
                let xs = ln.cells();
                let x0 = xs[seg.start] + v.xoff();
                if first || searched {
                    v.buf().text(p.line, &mut scratch);
                }
                if first && shows_text(&scratch, xs, seg, x0..x0 + right) {
                    let pos = Pos { line: p.line, col };
                    lines.push(Target {
                        y,
                        x: 0,
                        pos,
                        on_match: false,
                    });
                }
                if searched {
                    for k in v.matcher().all(&scratch) {
                        // A match may start on a mark or inside a
                        // cluster, which has no cell of its own.
                        let (i, _) = ln.glyph(k);
                        if (seg.start..seg.end).contains(&i) && (x0..x0 + right).contains(&xs[i]) {
                            let pos = Pos {
                                line: p.line,
                                col: k,
                            };
                            matches.push(Target {
                                y,
                                x: xs[i] - x0,
                                pos,
                                on_match: true,
                            });
                        }
                    }
                }
            }
            let next = v.next_row(p);
            if next == p {
                break;
            }
            p = next;
        }
        self.scratch = scratch;
        let targets = if matches.is_empty() { lines } else { matches };
        fit(targets, right, self.settings.hint_order)
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
        for (label, t) in self.hints() {
            for (i, c) in label.chars().enumerate() {
                g.put(t.x + i, t.y, c.encode_utf8(&mut [0; 4]), st, None);
            }
        }
    }
}
