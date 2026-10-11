//! -F's decision: print text that fits one screen instead of paging it.

use crate::text::buffer::Buffer;
use crate::view::layout::{Layout, Mode, WrapStyle};

/// -F's decision about the text read so far.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// The text fits so far but has not ended: it may grow.
    Undecided,
    /// It ended fitting the screen: print it; the pager never opens.
    Print,
    /// It outgrew the screen, or ended with a read error to show.
    Page,
}

/// The decision for the text `buf` holds so far, on a `w`×`h` screen:
/// text that at EOF fits with `below` rows left under it, for the
/// shell's prompt, is printed instead of paged. The last row is the
/// text's however many are asked. The printed text is the terminal's
/// to lay out: it wraps whatever the mode and its tabs stop every 8
/// cells whatever tab_width says, so it is measured that way. `seps`
/// counts a row for each input when there are several, as the pager
/// draws one.
pub fn one_page(buf: &Buffer, w: usize, h: usize, below: usize, seps: bool) -> Verdict {
    // EOF first: nothing appends between it and the measuring, so the
    // rows counted are the whole text's.
    let eof = match buf.finished() {
        Some(Err(_)) => return Verdict::Page,
        Some(Ok(())) => true,
        None => false,
    };
    let l = Layout {
        width: w,
        mode: Mode::Wrap,
        tab: 0,
        wrap_style: WrapStyle::Char,
    };
    let fit = h.saturating_sub(below).max(1);
    let seps = seps && buf.parts().len() > 1;
    let mut rows = 0;
    let mut text = Vec::new();
    for i in 0..buf.len() {
        if seps && buf.starts_part(i) {
            rows += 1;
        }
        buf.text(i, &mut text);
        rows += l.line(&text).rows();
        if rows > fit {
            return Verdict::Page;
        }
    }
    // A line whose bytes are gone from a file that shrank is found by
    // reading it, and is paged so it is seen.
    if buf.read_error().is_some() {
        return Verdict::Page;
    }
    if eof {
        Verdict::Print
    } else {
        Verdict::Undecided
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use super::*;
    use crate::text::buffer::ReadError;
    use crate::text::buffer::tests::{Fill, temp};

    fn reading(text: &str) -> Buffer {
        let mut f = Fill::memory(false);
        f.part();
        f.write(text.as_bytes());
        f.buf
    }

    fn read(text: &str) -> Buffer {
        let mut f = Fill::memory(false);
        f.part();
        f.write(text.as_bytes());
        f.finish(None);
        f.buf
    }

    #[test]
    fn undecided_before_eof() {
        assert_eq!(
            one_page(&reading("1\n2\n"), 40, 3, 1, false),
            Verdict::Undecided
        );
    }

    /// Text that outgrows the screen is paged as soon as it does, before EOF.
    #[test]
    fn pages_when_too_long() {
        assert_eq!(
            one_page(&reading("1\n2\n3\n"), 40, 3, 1, false),
            Verdict::Page
        );
    }

    #[test]
    fn prints_when_fits() {
        assert_eq!(one_page(&read("1\n2\n"), 40, 3, 1, false), Verdict::Print);
    }

    /// With several inputs the row naming each takes a row of the screen.
    #[test]
    fn counts_separators() {
        let mut f = Fill::memory(false);
        f.part();
        f.write(b"1\n");
        f.part();
        f.write(b"2\n");
        f.finish(None);
        assert_eq!(one_page(&f.buf, 40, 3, 1, true), Verdict::Page);
        assert_eq!(one_page(&f.buf, 40, 3, 1, false), Verdict::Print);
        assert_eq!(
            one_page(&read("1\n2\n"), 40, 3, 1, true),
            Verdict::Print,
            "no row names a lone input"
        );
    }

    #[test]
    fn pages_on_read_error() {
        let mut f = Fill::memory(false);
        f.part();
        f.write(b"1\n");
        f.finish(Some(ReadError::Io(std::sync::Arc::new(
            std::io::Error::other("disk on fire"),
        ))));
        assert_eq!(one_page(&f.buf, 40, 3, 1, false), Verdict::Page);
    }

    /// A file that shrank after it was read has no bytes for its lines.
    /// Measuring is what finds that, after the reader reported no
    /// error; -F pages so the error is shown rather than printing what
    /// is left.
    #[test]
    fn pages_on_error_found_measuring() {
        let path = temp(b"1\n2\n");
        let mut f = Fill::paged(vec![File::open(&path).unwrap()]);
        f.part();
        f.write(b"1\n2\n");
        f.finish(None);
        fs::write(&path, b"").unwrap();
        assert_eq!(one_page(&f.buf, 40, 3, 1, false), Verdict::Page);
        assert!(
            f.buf.read_error().is_some(),
            "the truncation was not found while measuring, so the test proves nothing"
        );
    }

    /// A one-row screen has no status line, so its one row is the text's.
    #[test]
    fn one_row_screen() {
        assert_eq!(one_page(&read("1\n"), 40, 1, 1, false), Verdict::Print);
    }

    /// The text fits when it leaves the rows asked for below it, and
    /// the last row is the text's however many are asked.
    #[test]
    fn rows_below() {
        let buf = read("1\n2\n");
        for (h, below, want) in [
            (4, 2, Verdict::Print),
            (3, 2, Verdict::Page),
            (3, 1, Verdict::Print),
            (2, 1, Verdict::Page),
        ] {
            assert_eq!(
                one_page(&buf, 40, h, below, false),
                want,
                "{h} rows leaving {below}"
            );
        }
        assert_eq!(one_page(&read("1\n"), 40, 2, 5, false), Verdict::Print);
    }

    /// -F measures the text as raw shows it, the escapes taking cells.
    #[test]
    fn view_style_raw() {
        let mut buf = read("\x1b[31mred\x1b[0m\n");
        assert_eq!(one_page(&buf, 10, 2, 1, true), Verdict::Print);
        buf.set_literal(true);
        assert_eq!(one_page(&buf, 10, 2, 1, true), Verdict::Page);
    }
}
