//! A grid of cells the app paints each frame, and a screen that sends
//! the terminal only the cells that changed.

use std::io;

use crate::text::ansi::Link;
use crate::view::layout::cluster_width;
use crate::view::style::Style;

#[derive(PartialEq, Eq, Debug)]
pub struct Cell {
    pub text: String,
    pub style: Style,
    pub link: Option<Link>,
    /// 1 or 2 for a cluster's first cell; 0 for the cell after a
    /// width-2 cluster.
    pub width: u8,
}

// By hand, so `clone_from` reuses the text's buffer: the screen copies
// every cell of every frame.
impl Clone for Cell {
    fn clone(&self) -> Cell {
        Cell {
            text: self.text.clone(),
            style: self.style,
            link: self.link.clone(),
            width: self.width,
        }
    }

    fn clone_from(&mut self, src: &Cell) {
        self.text.clone_from(&src.text);
        self.style = src.style;
        self.link.clone_from(&src.link);
        self.width = src.width;
    }
}

impl Cell {
    pub fn blank() -> Cell {
        Cell {
            text: " ".into(),
            style: Style::default(),
            link: None,
            width: 1,
        }
    }

    /// Makes this a blank in `style`, keeping the text's buffer.
    fn set_blank(&mut self, style: Style) {
        self.set(" ", style, None, 1);
    }

    fn set(&mut self, text: &str, style: Style, link: Option<Link>, width: u8) {
        self.text.clear();
        self.text.push_str(text);
        self.style = style;
        self.link = link;
        self.width = width;
    }
}

#[derive(Debug)]
pub struct Grid {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
    /// A zero-width cluster put at `(x, y)`, which the next put there
    /// takes in: layout gives such a cluster no cell of its own.
    pending: Option<(usize, usize, String)>,
}

// By hand, so `clone_from` reuses the cells' buffers.
impl Clone for Grid {
    fn clone(&self) -> Grid {
        Grid {
            w: self.w,
            h: self.h,
            cells: self.cells.clone(),
            pending: self.pending.clone(),
        }
    }

    fn clone_from(&mut self, src: &Grid) {
        self.w = src.w;
        self.h = src.h;
        self.cells.clone_from(&src.cells);
        self.pending.clone_from(&src.pending);
    }
}

impl Grid {
    pub fn new(w: usize, h: usize) -> Grid {
        Grid {
            w,
            h,
            cells: vec![Cell::blank(); w * h],
            pending: None,
        }
    }

    /// Blanks every cell, keeping their buffers.
    pub fn clear(&mut self) {
        for c in &mut self.cells {
            c.set_blank(Style::default());
        }
        self.pending = None;
    }

    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    pub fn get(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.w + x]
    }

    /// Draws one cluster at `(x, y)` and returns the cells it took: 0
    /// when it is zero-width or does not fit before the right edge.
    pub fn put(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        style: Style,
        link: Option<Link>,
    ) -> usize {
        if x >= self.w || y >= self.h {
            return 0;
        }
        let joined = match self.pending.take() {
            Some((px, py, mut p)) if (px, py) == (x, y) => {
                p.push_str(text);
                Some(p)
            }
            _ => None,
        };
        let width = cluster_width(joined.as_deref().unwrap_or(text));
        if width == 0 {
            self.pending = Some((x, y, joined.unwrap_or_else(|| text.to_owned())));
            return 0;
        }
        let text = joined.as_deref().unwrap_or(text);
        if x + width > self.w {
            return 0;
        }
        let i = y * self.w + x;
        // No half of a wide cluster may survive a put over its other half.
        if self.cells[i].width == 0 && x > 0 {
            self.cells[i - 1].set_blank(Style::default());
        }
        let last = i + width - 1;
        if self.cells[last].width == 2 && x + width < self.w {
            self.cells[last + 1].set_blank(Style::default());
        }
        if width == 2 {
            self.cells[i + 1].set("", style, link.clone(), 0);
        }
        self.cells[i].set(text, style, link, width as u8);
        width
    }

    /// Blanks row `y` in `style`.
    pub fn fill_row(&mut self, y: usize, style: Style) {
        for c in &mut self.cells[y * self.w..(y + 1) * self.w] {
            c.set_blank(style);
        }
    }

    /// Every cell with its `(x, y)`, row by row.
    pub fn cells(&self) -> impl Iterator<Item = (usize, usize, &Cell)> {
        let w = self.w;
        self.cells
            .iter()
            .enumerate()
            .map(move |(i, c)| (i % w, i / w, c))
    }
}

/// Where a `Screen` sends its frames.
pub trait Terminal {
    /// Columns and rows.
    fn size(&self) -> (usize, usize);
    /// Writes cells at their `(x, y)`, continuation cells included; after
    /// a resize a cell may lie past the terminal's edge.
    fn draw(&mut self, cells: &mut dyn Iterator<Item = (usize, usize, &Cell)>);
    /// Shows the cursor at a cell, or hides it for `None`.
    fn set_cursor(&mut self, at: Option<(usize, usize)>);
    /// Sends what was written.
    fn flush(&mut self) -> io::Result<()>;
    /// Writes bytes as they are, past the cells.
    fn write_raw(&mut self, bytes: &[u8]);
}

pub struct Screen<T: Terminal> {
    term: T,
    last: Option<Grid>,
}

impl<T: Terminal> Screen<T> {
    pub fn new(term: T) -> Screen<T> {
        Screen { term, last: None }
    }

    pub fn size(&self) -> (usize, usize) {
        self.term.size()
    }

    /// Shows `grid`, sending the cells that differ from the last frame,
    /// or all of them when the size differs.
    pub fn present(&mut self, grid: &Grid, cursor: Option<(usize, usize)>) {
        let last = self
            .last
            .as_ref()
            .filter(|l| (l.w, l.h) == (grid.w, grid.h));
        let changed = |x: usize, y: usize| last.is_none_or(|l| l.get(x, y) != grid.get(x, y));
        let mut cells = grid.cells().filter(|&(x, y, c)| {
            // A continuation cell is equal across clusters of the same
            // style, so it follows its first cell.
            changed(x, y) || (c.width == 0 && x > 0 && changed(x - 1, y))
        });
        self.term.draw(&mut cells);
        self.term.set_cursor(cursor);
        // A terminal that cannot be written to has gone; the app learns
        // that from its input ending, so the error has nothing to add.
        let _ = self.term.flush();
        match &mut self.last {
            Some(l) => l.clone_from(grid),
            None => self.last = Some(grid.clone()),
        }
    }

    /// Forgets the last frame, so the next `present` sends every cell.
    pub fn sync(&mut self) {
        self.last = None;
    }

    pub fn terminal(&self) -> &T {
        &self.term
    }

    pub fn terminal_mut(&mut self) -> &mut T {
        &mut self.term
    }
}

/// A terminal in memory, for tests.
pub struct TestTerminal {
    grid: Grid,
    cursor: Option<(usize, usize)>,
    raw: Vec<u8>,
    draws: Vec<(usize, usize)>,
}

impl TestTerminal {
    pub fn new(w: usize, h: usize) -> TestTerminal {
        TestTerminal {
            grid: Grid::new(w, h),
            cursor: None,
            raw: Vec::new(),
            draws: Vec::new(),
        }
    }

    /// Makes the terminal `w`×`h` and blank, as a real one's contents
    /// are undefined after a resize.
    pub fn resize(&mut self, w: usize, h: usize) {
        self.grid = Grid::new(w, h);
    }

    pub fn cell(&self, x: usize, y: usize) -> &Cell {
        self.grid.get(x, y)
    }

    pub fn cursor(&self) -> Option<(usize, usize)> {
        self.cursor
    }

    /// The bytes written raw since the last call.
    pub fn take_raw(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.raw)
    }

    /// The cells drawn since the last call.
    pub fn take_draws(&mut self) -> Vec<(usize, usize)> {
        std::mem::take(&mut self.draws)
    }
}

impl Terminal for TestTerminal {
    fn size(&self) -> (usize, usize) {
        (self.grid.w, self.grid.h)
    }

    fn draw(&mut self, cells: &mut dyn Iterator<Item = (usize, usize, &Cell)>) {
        for (x, y, c) in cells {
            if x < self.grid.w && y < self.grid.h {
                self.grid.cells[y * self.grid.w + x] = c.clone();
            }
            self.draws.push((x, y));
        }
    }

    fn set_cursor(&mut self, at: Option<(usize, usize)>) {
        self.cursor = at;
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        self.raw.extend_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::style::Style;

    fn put(g: &mut Grid, x: usize, y: usize, s: &str) -> usize {
        g.put(x, y, s, Style::default(), None)
    }

    #[test]
    fn present_sends_only_changed_cells() {
        let mut scr = Screen::new(TestTerminal::new(3, 2));
        let mut g = Grid::new(3, 2);
        put(&mut g, 0, 0, "a");
        scr.present(&g, Some((0, 0)));
        assert_eq!(scr.terminal_mut().take_draws().len(), 6);
        let mut g = Grid::new(3, 2);
        put(&mut g, 0, 0, "a");
        put(&mut g, 1, 1, "b");
        scr.present(&g, Some((1, 1)));
        assert_eq!(scr.terminal_mut().take_draws(), vec![(1, 1)]);
        assert_eq!(scr.terminal().cell(1, 1).text, "b");
        assert_eq!(scr.terminal().cell(0, 0).text, "a");
        assert_eq!(scr.terminal().cursor(), Some((1, 1)));
    }

    #[test]
    fn resize_redraws_all() {
        let mut scr = Screen::new(TestTerminal::new(2, 1));
        scr.present(&Grid::new(2, 1), None);
        scr.terminal_mut().take_draws();
        scr.terminal_mut().resize(3, 1);
        assert_eq!(scr.size(), (3, 1));
        scr.present(&Grid::new(3, 1), None);
        assert_eq!(
            scr.terminal_mut().take_draws(),
            vec![(0, 0), (1, 0), (2, 0)]
        );
        scr.present(&Grid::new(3, 1), None);
        assert!(scr.terminal_mut().take_draws().is_empty());
        scr.sync();
        scr.present(&Grid::new(3, 1), None);
        assert_eq!(scr.terminal_mut().take_draws().len(), 3);
    }

    #[test]
    fn wide_cluster_takes_two_cells_and_continuation() {
        let mut g = Grid::new(3, 1);
        assert_eq!(put(&mut g, 0, 0, "日"), 2);
        assert_eq!((g.get(0, 0).text.as_str(), g.get(0, 0).width), ("日", 2));
        assert_eq!((g.get(1, 0).text.as_str(), g.get(1, 0).width), ("", 0));
        assert_eq!(*g.get(2, 0), Cell::blank());

        let mut scr = Screen::new(TestTerminal::new(3, 1));
        scr.present(&g, None);
        scr.terminal_mut().take_draws();
        let mut g = Grid::new(3, 1);
        put(&mut g, 0, 0, "本");
        scr.present(&g, None);
        assert_eq!(scr.terminal_mut().take_draws(), vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn put_past_edge_draws_nothing() {
        let mut g = Grid::new(3, 1);
        assert_eq!(put(&mut g, 2, 0, "日"), 0);
        assert_eq!(put(&mut g, 3, 0, "a"), 0);
        assert_eq!(put(&mut g, 0, 1, "a"), 0);
        assert!(g.cells().all(|(_, _, c)| *c == Cell::blank()));
        assert_eq!(put(&mut g, 2, 0, "a"), 1);
    }

    #[test]
    fn one_by_one_grid_holds_one_cell() {
        let mut g = Grid::new(1, 1);
        assert_eq!((g.width(), g.height()), (1, 1));
        assert_eq!(put(&mut g, 0, 0, "日"), 0);
        assert_eq!(put(&mut g, 0, 0, "a"), 1);
        let cells: Vec<_> = g.cells().map(|(x, y, c)| (x, y, c.text.clone())).collect();
        assert_eq!(cells, vec![(0, 0, "a".to_string())]);
    }

    #[test]
    fn zero_width_folds_into_next_cell() {
        let mut g = Grid::new(3, 1);
        assert_eq!(put(&mut g, 0, 0, "\u{200b}"), 0);
        assert_eq!(*g.get(0, 0), Cell::blank());
        assert_eq!(put(&mut g, 0, 0, "e"), 1);
        assert_eq!(g.get(0, 0).text, "\u{200b}e");
        // Pending text goes only to the next put, and only at its cell.
        put(&mut g, 1, 0, "\u{200b}");
        put(&mut g, 2, 0, "x");
        assert_eq!(g.get(2, 0).text, "x");
        put(&mut g, 1, 0, "y");
        assert_eq!(g.get(1, 0).text, "y");
    }

    #[test]
    fn overwriting_half_a_wide_cluster_blanks_the_other_half() {
        let mut g = Grid::new(4, 1);
        put(&mut g, 0, 0, "日");
        put(&mut g, 1, 0, "a");
        assert_eq!(*g.get(0, 0), Cell::blank());
        assert_eq!(g.get(1, 0).text, "a");

        put(&mut g, 0, 0, "日");
        put(&mut g, 0, 0, "b");
        assert_eq!((g.get(0, 0).text.as_str(), g.get(0, 0).width), ("b", 1));
        assert_eq!(*g.get(1, 0), Cell::blank());

        put(&mut g, 0, 0, "日");
        put(&mut g, 1, 0, "本");
        assert_eq!(*g.get(0, 0), Cell::blank());
        assert_eq!((g.get(1, 0).text.as_str(), g.get(2, 0).width), ("本", 0));

        put(&mut g, 0, 0, "月");
        assert_eq!((g.get(0, 0).text.as_str(), g.get(1, 0).width), ("月", 0));
        assert_eq!(*g.get(2, 0), Cell::blank());
    }

    #[test]
    fn fill_row_paints_blanks() {
        let mut g = Grid::new(2, 2);
        put(&mut g, 0, 1, "日");
        let st = Style::default().reverse();
        g.fill_row(1, st);
        for x in 0..2 {
            assert_eq!((g.get(x, 1).text.as_str(), g.get(x, 1).style), (" ", st));
            assert_eq!(g.get(x, 1).width, 1);
        }
        assert_eq!(*g.get(0, 0), Cell::blank());
    }

    #[test]
    fn clear_blanks_every_cell() {
        let mut g = Grid::new(3, 2);
        put(&mut g, 0, 0, "日");
        put(&mut g, 2, 1, "x");
        g.fill_row(1, Style::default().reverse());
        put(&mut g, 2, 0, "\u{301}");
        assert!(g.pending.is_some());
        g.clear();
        assert!(g.cells().all(|(_, _, c)| *c == Cell::blank()));
        put(&mut g, 2, 0, "e");
        assert_eq!(g.get(2, 0).text, "e", "pending zero-width text dropped");
    }
}
