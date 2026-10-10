//! Encodes cells as terminal escapes.

use std::io::Write;

use crate::text::ansi::Link;
use crate::view::screen::Cell;
use crate::view::style::{Attrs, Color, Style};

/// Writes cells, tracking the terminal's pen, style and link so each
/// escape is sent only when it changes.
pub struct Renderer {
    pen: Option<(usize, usize)>,
    /// `None` while the terminal's style is unknown.
    style: Option<Style>,
    link: Option<Link>,
}

impl Default for Renderer {
    fn default() -> Renderer {
        Renderer::new()
    }
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            pen: None,
            style: None,
            link: None,
        }
    }

    pub fn cell(&mut self, out: &mut Vec<u8>, x: usize, y: usize, cell: &Cell) {
        if cell.width == 0 {
            return;
        }
        if self.pen != Some((x, y)) {
            let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
        }
        if self.style != Some(cell.style) {
            sgr(out, cell.style);
            self.style = Some(cell.style);
        }
        // A link with no URL is none, as the parser treats one. A control
        // character would end the OSC 8 early and run what follows it.
        let link = cell.link.as_ref().filter(|l| {
            !l.url.is_empty() && !l.url.chars().chain(l.id.chars()).any(char::is_control)
        });
        if self.link.as_ref() != link {
            match link {
                Some(l) if l.id.is_empty() => {
                    let _ = write!(out, "\x1b]8;;{}\x1b\\", l.url);
                }
                Some(l) => {
                    let _ = write!(out, "\x1b]8;id={};{}\x1b\\", l.id, l.url);
                }
                None => out.extend_from_slice(b"\x1b]8;;\x1b\\"),
            }
            self.link = link.cloned();
        }
        out.extend_from_slice(cell.text.as_bytes());
        self.pen = Some((x + usize::from(cell.width), y));
    }

    /// Closes any open link and resets the style. The pen and style are
    /// then unknown, as whatever runs next (a suspend, a shell) may move
    /// or restyle the terminal.
    pub fn reset(&mut self, out: &mut Vec<u8>) {
        if self.link.take().is_some() {
            out.extend_from_slice(b"\x1b]8;;\x1b\\");
        }
        out.extend_from_slice(b"\x1b[0m");
        self.pen = None;
        self.style = None;
    }

    /// Tells the renderer the terminal's cursor was moved behind its
    /// back, so the next cell is placed explicitly.
    pub fn cursor_moved(&mut self) {
        self.pen = None;
    }
}

/// Sets `style` whole, from a reset: simpler than a delta, and as short
/// for the few attributes a pager's text carries.
fn sgr(out: &mut Vec<u8>, style: Style) {
    out.extend_from_slice(b"\x1b[0");
    for (attr, code) in [
        (Attrs::BOLD, 1),
        (Attrs::DIM, 2),
        (Attrs::ITALIC, 3),
        (Attrs::UNDERLINE, 4),
        (Attrs::REVERSE, 7),
        (Attrs::STRIKE, 9),
    ] {
        if style.attrs.contains(attr) {
            let _ = write!(out, ";{code}");
        }
    }
    color(out, style.fg, 30, 90, 38);
    color(out, style.bg, 40, 100, 48);
    out.push(b'm');
}

fn color(out: &mut Vec<u8>, c: Color, base: u8, bright: u8, ext: u8) {
    let _ = match c {
        Color::Default => Ok(()),
        Color::Palette(n @ 0..8) => write!(out, ";{}", base + n),
        Color::Palette(n @ 8..16) => write!(out, ";{}", bright + n - 8),
        Color::Palette(n) => write!(out, ";{ext};5;{n}"),
        Color::Rgb(r, g, b) => write!(out, ";{ext};2;{r};{g};{b}"),
    };
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn cell(text: &str, style: Style) -> Cell {
        Cell {
            text: text.into(),
            style,
            link: None,
            width: 1,
        }
    }

    fn s(out: &[u8]) -> &str {
        std::str::from_utf8(out).unwrap()
    }

    #[test]
    fn sgr_written_once_per_style_change() {
        let mut r = Renderer::new();
        let mut out = Vec::new();
        let bold = Style::default().bold();
        let red = Style::default().fg(Color::Palette(1));
        r.cell(&mut out, 0, 0, &cell("a", bold));
        r.cell(&mut out, 1, 0, &cell("b", bold));
        r.cell(&mut out, 2, 0, &cell("c", red));
        r.cell(&mut out, 3, 0, &cell("d", red));
        r.cell(&mut out, 4, 0, &cell("e", Style::default()));
        assert_eq!(s(&out), "\x1b[1;1H\x1b[0;1mab\x1b[0;31mcd\x1b[0me");
    }

    #[test]
    fn sgr_encodes_colors_and_attrs() {
        let mut r = Renderer::new();
        let mut out = Vec::new();
        let st = Style::default()
            .fg(Color::Palette(9))
            .bg(Color::Palette(4))
            .with(Attrs::DIM | Attrs::ITALIC | Attrs::UNDERLINE | Attrs::REVERSE | Attrs::STRIKE);
        r.cell(&mut out, 0, 0, &cell("a", st));
        let st = Style::default()
            .fg(Color::Rgb(1, 2, 3))
            .bg(Color::Palette(200));
        r.cell(&mut out, 1, 0, &cell("b", st));
        let st = Style::default()
            .fg(Color::Palette(16))
            .bg(Color::Rgb(4, 5, 6));
        r.cell(&mut out, 2, 0, &cell("c", st.bg(Color::Palette(15))));
        r.cell(&mut out, 3, 0, &cell("d", st));
        assert_eq!(
            s(&out),
            "\x1b[1;1H\x1b[0;2;3;4;7;9;91;44ma\
             \x1b[0;38;2;1;2;3;48;5;200mb\
             \x1b[0;38;5;16;107mc\
             \x1b[0;38;5;16;48;2;4;5;6md"
        );
    }

    #[test]
    fn osc8_wraps_linked_cells() {
        let link = |id: &str| {
            Some(Link {
                url: Arc::from("http://x"),
                id: Arc::from(id),
            })
        };
        let mut r = Renderer::new();
        let mut out = Vec::new();
        let mut a = cell("a", Style::default());
        a.link = link("k");
        r.cell(&mut out, 0, 0, &a);
        r.cell(&mut out, 1, 0, &cell("b", Style::default()));
        assert_eq!(
            s(&out),
            "\x1b[1;1H\x1b[0m\x1b]8;id=k;http://x\x1b\\a\x1b]8;;\x1b\\b"
        );

        out.clear();
        a.link = link("");
        r.cell(&mut out, 2, 0, &a);
        a.link = link("j");
        r.cell(&mut out, 3, 0, &a);
        r.reset(&mut out);
        assert_eq!(
            s(&out),
            "\x1b]8;;http://x\x1b\\a\x1b]8;id=j;http://x\x1b\\a\x1b]8;;\x1b\\\x1b[0m"
        );

        out.clear();
        a.link = Some(Link {
            url: Arc::from(""),
            id: Arc::from("k"),
        });
        r.cell(&mut out, 4, 0, &a);
        assert_eq!(s(&out), "\x1b[1;5H\x1b[0ma");
    }

    #[test]
    fn no_osc8_for_a_link_holding_a_control() {
        let mut r = Renderer::new();
        let mut out = Vec::new();
        let mut a = cell("a", Style::default());
        a.link = Some(Link {
            url: Arc::from("http://x\x1b]52;c;aGk="),
            id: Arc::from(""),
        });
        r.cell(&mut out, 0, 0, &a);
        a.link = Some(Link {
            url: Arc::from("http://x"),
            id: Arc::from("k\u{9c}"),
        });
        r.cell(&mut out, 1, 0, &a);
        assert_eq!(s(&out), "\x1b[1;1H\x1b[0maa");
    }

    #[test]
    fn cursor_moves_only_when_skipping() {
        let mut r = Renderer::new();
        let mut out = Vec::new();
        let st = Style::default();
        r.cell(&mut out, 0, 0, &cell("a", st));
        r.cell(&mut out, 1, 0, &cell("b", st));
        r.cell(&mut out, 3, 0, &cell("c", st));
        r.cell(&mut out, 0, 1, &cell("d", st));
        let wide = Cell {
            text: "日".into(),
            style: st,
            link: None,
            width: 2,
        };
        let cont = Cell {
            text: String::new(),
            style: st,
            link: None,
            width: 0,
        };
        r.cell(&mut out, 1, 1, &wide);
        r.cell(&mut out, 2, 1, &cont);
        r.cell(&mut out, 3, 1, &cell("e", st));
        assert_eq!(s(&out), "\x1b[1;1H\x1b[0mab\x1b[1;4Hc\x1b[2;1Hd日e");
    }

    #[test]
    fn cursor_moved_forces_a_move_next_cell() {
        let mut r = Renderer::new();
        let mut out = Vec::new();
        let st = Style::default();
        r.cell(&mut out, 0, 0, &cell("a", st));
        r.cursor_moved();
        r.cell(&mut out, 1, 0, &cell("b", st));
        assert_eq!(s(&out), "\x1b[1;1H\x1b[0ma\x1b[1;2Hb");
    }

    #[test]
    fn reset_forgets_pen_and_style() {
        let mut r = Renderer::new();
        let mut out = Vec::new();
        let bold = Style::default().bold();
        r.cell(&mut out, 0, 0, &cell("a", bold));
        r.reset(&mut out);
        r.cell(&mut out, 1, 0, &cell("b", bold));
        assert_eq!(s(&out), "\x1b[1;1H\x1b[0;1ma\x1b[0m\x1b[1;2H\x1b[0;1mb");
    }
}
