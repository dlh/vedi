//! The pager: its state, and the loop that drives it from events.

pub mod event;
pub mod onepage;
pub mod options;

mod command;
mod copy;
mod draw;
mod help;
mod motion;
mod mouse;
mod prompt;
mod reload;
mod search;
mod selection;
mod status;
#[cfg(test)]
mod tests;

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub use event::{Event, Mouse, MouseKind};
pub use onepage::{Verdict, one_page};
pub use options::{Clock, Copier, Opener, Options, STDIN, StartScreen};

use crate::input::action::{Action, Command};
use crate::input::keymap::Keymap;
use crate::text::ansi::ViewStyle;
use crate::text::buffer::{Buffer, Pos};
use crate::text::reader::Gen;
use crate::view::layout::{self, Layout, WrapStyle};
use crate::view::screen::{Grid, Screen, Terminal};
use motion::{LaidCache, Row};

/// How often the reader's data is drawn, at most.
const REDRAW_EVERY: Duration = Duration::from_millis(50);

/// How many lines are kept laid out before starting over.
const LAID_LINES: usize = 1024;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Flow {
    Continue,
    Quit,
}

/// What a key drives.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Focus {
    Viewing,
    /// The / prompt, or ? when `back`.
    Searching {
        back: bool,
    },
    /// The : prompt.
    Commanding,
    /// The key bindings are shown instead of the text.
    Helping,
}

pub struct App<T: Terminal> {
    screen: Screen<T>,
    /// The last frame drawn, whose cells the next one reuses.
    frame: Grid,
    buf: Buffer,
    /// The reader's generation; progress from any other is dropped.
    generation: Gen,
    copier: Box<dyn Copier>,
    clock: Clock,
    names: Vec<String>,
    keys: Keymap,
    wrap_mode: layout::Mode,
    wrap_style: WrapStyle,
    /// Cells per tab stop; 0 for the default.
    tab: usize,
    /// Mark text off the sides with < and >, a wrapped row with \.
    marks: bool,
    /// A row names each input, when there are several.
    seps: bool,
    /// A `Changed` event reads the file again.
    auto: bool,
    /// The status line shows only when it has something to say.
    hide: bool,
    view: ViewStyle,

    cur: Pos,
    /// The selection anchor; `None` when there is no selection.
    anchor: Option<Pos>,
    /// The mark is set: motions extend the selection.
    marking: bool,
    /// The first visible row.
    top: Row,
    /// Horizontal scroll in `NoWrap` mode.
    xoff: usize,

    follow: bool,
    /// The 0-based +N target, until applied.
    start_line: Option<usize>,
    /// Applied at EOF; no text is drawn until then.
    start_screen: Option<StartScreen>,
    /// A one-shot message, cleared by the next key.
    status: Option<String>,
    focus: Focus,
    search: search::State,
    /// The : prompt and its completion.
    command: command::State,
    /// The text and its view, set aside while help is up.
    text: Option<help::TextView>,
    mouse: mouse::State,
    /// `None` when the input cannot be read again.
    open: Option<Opener>,
    /// A reload being read.
    next: Option<reload::Pending>,
    /// A change came during the reload: one more after it.
    dirty: bool,

    laid: LaidCache,
    /// A line's chars, decoded into it rather than allocated per call.
    scratch: Vec<char>,
    /// When a draw last took up the reader's data, which the hold runs
    /// from: a frame drawn before any, as the first, holds nothing back.
    last_draw: Option<Instant>,
    /// Progress arrived since the last draw.
    pending: bool,
}

impl<T: Terminal> App<T> {
    /// Takes `buffer` as read so far: `generation` is its reader's.
    pub fn new(term: T, mut buffer: Buffer, generation: Gen, opts: Options) -> App<T> {
        buffer.set_literal(opts.view_style == ViewStyle::Raw);
        let mut a = App {
            screen: Screen::new(term),
            frame: Grid::new(0, 0),
            buf: buffer,
            generation,
            copier: opts.copier,
            clock: opts.clock,
            names: opts.names,
            keys: opts.keys,
            wrap_mode: opts.mode,
            wrap_style: opts.wrap_style,
            tab: opts.tab_width,
            marks: opts.edge_markers,
            seps: opts.file_separators,
            auto: opts.auto_reload,
            hide: opts.hide_status,
            view: opts.view_style,
            cur: Pos::default(),
            anchor: None,
            marking: false,
            // The first row: line 0's separator, if it has one.
            top: Row::sep(0),
            xoff: 0,
            follow: opts.follow,
            start_line: opts.start_line.filter(|&n| n > 0).map(|n| n - 1),
            start_screen: opts.screen,
            status: None,
            focus: Focus::Viewing,
            search: search::State::new(),
            command: command::State::default(),
            text: None,
            mouse: mouse::State::default(),
            open: opts.open,
            next: None,
            dirty: false,
            laid: LaidCache::new(Layout::default()),
            scratch: Vec::new(),
            last_draw: None,
            pending: false,
        };
        a.on_data();
        a
    }

    pub fn terminal(&self) -> &T {
        self.screen.terminal()
    }

    pub fn terminal_mut(&mut self) -> &mut T {
        self.screen.terminal_mut()
    }

    pub fn cursor(&self) -> Pos {
        self.cur
    }

    /// Draws and handles events until an action quits or `rx` closes.
    /// The reader's data is drawn at most every `REDRAW_EVERY` until
    /// EOF, so a fast reader does not starve the keys.
    pub fn run(&mut self, rx: Receiver<Event>) {
        self.draw();
        loop {
            let mut ev = match self.deadline() {
                Some(d) => {
                    let now = (self.clock)();
                    match rx.recv_timeout(d.saturating_duration_since(now)) {
                        Ok(ev) => ev,
                        Err(RecvTimeoutError::Timeout) => Event::Tick,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                None => match rx.recv() {
                    Ok(ev) => ev,
                    Err(_) => return,
                },
            };
            loop {
                let progress = matches!(ev, Event::Progress(..));
                if self.handle(ev) == Flow::Quit {
                    return;
                }
                if !progress {
                    self.draw();
                    break;
                }
                match rx.try_recv() {
                    Ok(next) => ev = next,
                    Err(_) => {
                        if self.redraw_due() {
                            self.draw();
                        }
                        break;
                    }
                }
            }
        }
    }

    /// When the loop must wake without an event: the sooner of the
    /// redraw hold's end and the auto-scroll tick.
    pub fn deadline(&self) -> Option<Instant> {
        let hold = if self.pending && self.reading_buf().finished().is_none() {
            self.last_draw.map(|t| t + REDRAW_EVERY)
        } else {
            None
        };
        match (hold, self.mouse.ticking) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The buffer a reader is filling: a reload's, else the text's. The
    /// text's reader may still be at work during a reload; its data is
    /// then drawn at most the hold late.
    fn reading_buf(&self) -> &Buffer {
        match &self.next {
            Some(p) => &p.buf,
            None => self.text_buf(),
        }
    }

    fn redraw_due(&mut self) -> bool {
        match self.deadline() {
            Some(d) => (self.clock)() >= d,
            None => true,
        }
    }

    /// Processes one event and reports whether to quit.
    pub fn handle(&mut self, ev: Event) -> Flow {
        let flow = self.dispatch(ev);
        self.uncover();
        flow
    }

    fn dispatch(&mut self, ev: Event) -> Flow {
        match ev {
            Event::Progress(g, p) => {
                if g == self.generation {
                    self.text_buf_mut().apply(p);
                } else if let Some(n) = &mut self.next
                    && n.generation == g
                {
                    n.buf.apply(p);
                } else {
                    return Flow::Continue;
                }
                self.pending = true;
                self.swap_if_done();
                self.on_data();
            }
            Event::Resize(..) => {
                self.screen.sync();
                if self.text.is_some() {
                    self.scroll_help(0);
                    self.scroll_help_sideways(0);
                } else {
                    self.scroll_to_cursor();
                }
            }
            Event::Key(k) => {
                self.act();
                return match self.focus {
                    Focus::Viewing => {
                        let cmd = self.keys.lookup(k);
                        self.view_key(cmd)
                    }
                    Focus::Searching { back } => {
                        self.search_key(k, back);
                        Flow::Continue
                    }
                    Focus::Commanding => self.command_key(k),
                    Focus::Helping => {
                        let cmd = self.keys.lookup(k);
                        self.help_key(cmd);
                        Flow::Continue
                    }
                };
            }
            Event::Mouse(m) => self.handle_mouse(m),
            Event::Tick => self.tick(),
            Event::Changed => {
                if self.auto {
                    self.reload(false);
                }
            }
        }
        Flow::Continue
    }

    /// Records that the user did something: startup positioning stops
    /// and the one-shot status clears.
    fn act(&mut self) {
        self.follow = false;
        self.start_line = None;
        self.start_screen = None;
        self.status = None;
    }

    fn view_key(&mut self, cmd: Option<Command>) -> Flow {
        let Some(c) = cmd else {
            self.scroll_to_cursor();
            return Flow::Continue;
        };
        if c.action.is_movement() {
            if c.extend {
                self.extend();
            } else {
                self.drop();
            }
        }
        // Rows a row motion had nowhere to take.
        let mut left = 0;
        match c.action {
            Action::Up => left = self.move_rows(-1),
            Action::Down => left = self.move_rows(1),
            Action::Left => self.move_col(-1),
            Action::Right => self.move_col(1),
            Action::Home => self.cur.col = 0,
            Action::End => self.cur.col = self.line_len(self.cur.line),
            Action::PageUp => left = self.move_rows(-(self.page_rows() as isize)),
            Action::PageDown => left = self.move_rows(self.page_rows() as isize),
            Action::HalfPageUp => left = self.move_rows(-(self.half_page_rows() as isize)),
            Action::HalfPageDown => left = self.move_rows(self.half_page_rows() as isize),
            Action::WordLeft => self.word_left(),
            Action::WordRight => self.word_right(),
            Action::First => self.cur = Pos::default(),
            Action::Last => {
                self.cur = Pos {
                    line: self.buf.len().saturating_sub(1),
                    col: 0,
                }
            }
            Action::PrevFile => self.prev_input(),
            Action::NextFile => self.next_input(),
            Action::SelectAll => self.select_all(),
            Action::ClearSelection => self.clear_selection(),
            Action::SetMark => self.set_mark(),
            Action::Copy => {
                self.copy();
            }
            Action::CopyAndQuit => {
                if self.selection().is_some() {
                    return if self.copy() {
                        Flow::Quit
                    } else {
                        Flow::Continue
                    };
                }
                self.drop();
                self.move_rows(1);
            }
            Action::Search | Action::SearchBack => self.open_search(c.action == Action::SearchBack),
            Action::SearchNext => self.find(self.search.backward, true),
            Action::SearchPrev => self.find(!self.search.backward, true),
            Action::CommandPrompt => self.open_command(),
            Action::Cycle => self.cycle(c.arg.as_deref().unwrap_or_default()),
            Action::Help => self.show_help(),
            Action::Reload => self.reload(true),
            Action::Quit => return Flow::Quit,
        }
        // Extending past the last row, by Shift or the mark, takes the
        // rest of the last line; past the first, the start of the first.
        let extending = c.extend || self.marking;
        if extending && left > 0 {
            self.cur = self.end_pos();
        } else if extending && left < 0 {
            self.cur = Pos::default();
        }
        self.scroll_to_cursor();
        Flow::Continue
    }

    /// Whether wrap mode marks rows that continue, on a screen `w` wide:
    /// the last column is then the marker's, and text wraps a column
    /// early. Under three columns there is no room.
    fn wrap_marks(&self, w: usize) -> bool {
        self.marks && self.wrap_mode == layout::Mode::Wrap && w > 2
    }

    fn layout(&self) -> Layout {
        let (w, _) = self.screen.size();
        let width = if self.wrap_marks(w) { w - 1 } else { w };
        Layout {
            width,
            mode: self.wrap_mode,
            tab: self.tab,
            wrap_style: self.wrap_style,
        }
    }

    /// The rows left for text: all but the status line, which needs two
    /// rows to exist, or all of them while it is hidden.
    fn text_rows(&self) -> usize {
        let (_, h) = self.screen.size();
        if !self.status_shown() || h < 2 {
            return h;
        }
        h - 1
    }
}
