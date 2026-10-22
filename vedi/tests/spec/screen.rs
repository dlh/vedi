use vedi::view::screen::{Terminal, TestTerminal};
use vedi::view::style::{Attrs, CUR_MATCH, MATCH};

/// Renders the screen as a "-- screen --" section expects it:
/// reverse-video runs in brackets, search matches in braces, doubled
/// for the matches the cursor is in, the status row, when it is shown
/// in reverse video, plain with its runs out of reverse video in
/// brackets, a prompt's row as the text rows, trailing spaces trimmed,
/// one line per row.
pub fn render(term: &TestTerminal, status_shown: bool) -> String {
    let (w, h) = term.size();
    let mut out = String::new();
    for y in 0..h {
        let mut row = String::new();
        let status = status_shown
            && h >= 2
            && w > 0
            && y == h - 1
            && term.cell(0, y).style.attrs.contains(Attrs::REVERSE);
        let (mut rev, mut matched) = (false, "");
        let mut x = 0;
        while x < w {
            let c = term.cell(x, y);
            // A wide cell's second half holds nothing of its own.
            x += usize::from(c.width.max(1));
            let r = c.style.attrs.contains(Attrs::REVERSE) != status;
            let m = match (status, c.link.is_some()) {
                (true, _) | (_, true) => "",
                _ if c.style == MATCH => "{",
                _ if c.style == CUR_MATCH => "{{",
                _ => "",
            };
            if r && !rev {
                row.push('[');
            } else if !r && rev {
                row.push(']');
            }
            if m != matched {
                row.push_str(&"}".repeat(matched.len()));
                row.push_str(m);
            }
            rev = r;
            matched = m;
            row.push_str(&c.text);
        }
        if rev {
            row.push(']');
        }
        row.push_str(&"}".repeat(matched.len()));
        out.push_str(row.trim_end_matches(' '));
        out.push('\n');
    }
    out
}

/// Lists `want` and `got` rows, marking the ones that differ.
pub fn side_by_side(want: &str, got: &str) -> String {
    let w: Vec<&str> = want
        .strip_suffix('\n')
        .unwrap_or(want)
        .split('\n')
        .collect();
    let g: Vec<&str> = got.strip_suffix('\n').unwrap_or(got).split('\n').collect();
    let mut out = String::new();
    for i in 0..w.len().max(g.len()) {
        let a = w.get(i).copied().unwrap_or("");
        let b = g.get(i).copied().unwrap_or("");
        let mark = if a == b { ' ' } else { '!' };
        out.push_str(&format!(
            "{mark} {i:2} want {:<40} got {b:?}\n",
            format!("{a:?}")
        ));
    }
    out
}
