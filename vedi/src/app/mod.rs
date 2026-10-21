//! The pager: its state, and the loop that drives it from events.

pub mod event;
pub mod onepage;
pub mod options;

mod command;
mod copy;
mod draw;
mod help;
mod mouse;
mod prompt;
mod reload;
mod search;
mod status;
#[cfg(test)]
mod tests;
mod text_view;

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub use event::{Event, Mouse, MouseKind};
pub use onepage::{Verdict, one_page};
pub use options::{
    Clock, Copier, CopyResult, Done, OpenCmdFor, Opener, Options, STDIN, StartScreen, Wake,
};

use crate::input::action::{Action, Command};
use crate::input::keymap::Keymap;
use crate::input::setting::Settings;
use crate::text::ansi::ViewStyle;
use crate::text::buffer::{Buffer, Pos, Progress};
use crate::text::reader::Gen;
use crate::view::layout::Mode;
use crate::view::screen::{Grid, Screen, Terminal};
use prompt::Prompt;
use text_view::{Geometry, Motion, Start, TextView};

/// How often the reader's data is drawn, at most.
const REDRAW_EVERY: Duration = Duration::from_millis(50);

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
}

pub struct App<T: Terminal> {
    screen: Screen<T>,
    /// The last frame drawn, whose cells the next one reuses.
    frame: Grid,
    /// The text and its view; the bindings' while help is up.
    view: TextView,
    /// The text and its view, set aside while the bindings are shown.
    help: Option<TextView>,
    /// The reader's generation; progress from any other is dropped.
    generation: Gen,
    /// The last generation handed out: a dropped reload's is never
    /// reused, so its reader's data stays dropped.
    last_gen: Gen,
    copier: Box<dyn Copier>,
    /// A copy whose result is still to come.
    copying: Option<copy::Copying>,
    wake: Wake,
    clock: Clock,
    names: Vec<String>,
    keys: Keymap,
    settings: Settings,
    /// A one-shot message, cleared by the next key.
    status: Option<String>,
    focus: Focus,
    /// The / and ? prompts' line.
    search: Prompt,
    /// The : prompt and its completion.
    command: command::State,
    mouse: mouse::State,
    /// `None` when the input cannot be read again.
    open: Option<Opener>,
    open_cmd: Option<OpenCmdFor>,
    /// Off, a reload reads the files as they are, not through open_cmd.
    through_open_cmd: bool,
    /// A reload being read.
    next: Option<reload::Pending>,
    /// A change came during the reload: one more after it.
    dirty: bool,

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
        buffer.set_literal(opts.settings.view_style == ViewStyle::Raw);
        let start = Start {
            line: opts.start_line.filter(|&n| n > 0).map(|n| n - 1),
            follow: opts.follow,
            screen: opts.screen,
        };
        let mut a = App {
            screen: Screen::new(term),
            frame: Grid::new(0, 0),
            view: TextView::new(buffer, start, Geometry::default()),
            help: None,
            generation,
            last_gen: generation,
            copier: opts.copier,
            copying: None,
            wake: opts.wake,
            clock: opts.clock,
            names: opts.names,
            keys: opts.keys,
            settings: opts.settings,
            status: None,
            focus: Focus::Viewing,
            search: Prompt::default(),
            command: command::State::default(),
            mouse: mouse::State::default(),
            open: opts.open,
            open_cmd: opts.open_cmd,
            through_open_cmd: true,
            next: None,
            dirty: false,
            scratch: Vec::new(),
            last_draw: None,
            pending: false,
        };
        a.sync_view();
        a.view.appended();
        a
    }

    pub fn terminal(&self) -> &T {
        self.screen.terminal()
    }

    pub fn terminal_mut(&mut self) -> &mut T {
        self.screen.terminal_mut()
    }

    pub fn cursor(&self) -> Pos {
        self.view.cursor()
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
            // Queued progress is taken up once: a source that writes a
            // line at a time would otherwise decode the last block per
            // line.
            let mut applied = false;
            let ev = loop {
                let Event::Progress(g, p) = ev else {
                    break Some(ev);
                };
                applied |= self.apply(g, p);
                match rx.try_recv() {
                    Ok(next) => ev = next,
                    Err(_) => break None,
                }
            };
            if applied {
                self.take_up();
                self.sync_view();
            }
            match ev {
                Some(ev) => {
                    if self.handle(ev) == Flow::Quit {
                        return;
                    }
                    self.draw();
                }
                None => {
                    if self.redraw_due() {
                        self.draw();
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
            None => self.text_view().buf(),
        }
    }

    fn redraw_due(&mut self) -> bool {
        match self.deadline() {
            Some(d) => (self.clock)() >= d,
            None => true,
        }
    }

    /// Applies the reader's progress to its buffer and reports whether
    /// it was taken: a stale generation's is dropped.
    fn apply(&mut self, g: Gen, p: Progress) -> bool {
        if g == self.generation {
            self.text_view_mut().apply(p);
        } else if let Some(n) = &mut self.next
            && n.generation == g
        {
            n.buf.apply(p);
        } else {
            return false;
        }
        self.pending = true;
        true
    }

    /// Takes applied progress up: swaps in a finished reload and places
    /// the view on the new text.
    fn take_up(&mut self) {
        self.swap_if_done();
        self.text_view_mut().appended();
    }

    /// Processes one event and reports whether to quit.
    pub fn handle(&mut self, ev: Event) -> Flow {
        self.sync_view();
        let flow = self.dispatch(ev);
        self.sync_view();
        flow
    }

    /// The text and its view: the one shown, or the one help set aside.
    pub(super) fn text_view(&self) -> &TextView {
        self.help.as_ref().unwrap_or(&self.view)
    }

    pub(super) fn text_view_mut(&mut self) -> &mut TextView {
        self.help.as_mut().unwrap_or(&mut self.view)
    }

    /// Gives the keys to `f`; the status line comes or goes with it.
    fn focus(&mut self, f: Focus) {
        self.focus = f;
        self.sync_view();
    }

    /// Lays the view shown out for the screen as it is: the rows left
    /// by the status line, the settings, and `NoWrap` for help.
    fn sync_view(&mut self) {
        let g = self.geometry();
        self.view.resize(g);
    }

    fn geometry(&self) -> Geometry {
        let (cols, height) = self.screen.size();
        Geometry {
            rows: self.text_rows(),
            height,
            cols,
            mode: if self.help.is_some() {
                Mode::NoWrap
            } else {
                self.settings.mode
            },
            tab: self.settings.tab_width,
            wrap_style: self.settings.wrap_style,
            separators: self.settings.file_separators && self.names.len() > 1,
            edge_markers: self.settings.edge_markers,
        }
    }

    fn dispatch(&mut self, ev: Event) -> Flow {
        match ev {
            Event::Progress(g, p) => {
                if self.apply(g, p) {
                    self.take_up();
                }
            }
            Event::Resize(..) => {
                self.screen.sync();
                self.sync_view();
                // The bindings keep the screen full and the scroll
                // within them.
                if self.help.is_some() {
                    self.view.scroll_within(0);
                    self.view.scroll_sideways_within(0);
                }
            }
            Event::Key(k) => {
                self.act();
                return match self.focus {
                    Focus::Viewing if self.help.is_some() => {
                        let cmd = self.keys.lookup(k);
                        self.help_key(cmd);
                        Flow::Continue
                    }
                    Focus::Viewing => match self.keys.lookup(k) {
                        Some(c) => self.view_key(c),
                        None => {
                            self.status = Some(format!("not bound: {k}"));
                            Flow::Continue
                        }
                    },
                    Focus::Searching { back } => {
                        self.search_key(k, back);
                        Flow::Continue
                    }
                    Focus::Commanding => self.command_key(k),
                };
            }
            Event::Paste(text) => match self.focus {
                Focus::Searching { .. } => self.search.paste(&text),
                Focus::Commanding => self.command.paste(&text),
                Focus::Viewing => {}
            },
            Event::Mouse(m) => self.handle_mouse(m),
            Event::Tick => self.tick(),
            Event::Copied => self.copy_done(),
            Event::Changed => {
                if self.settings.auto_reload {
                    self.reload(false);
                }
            }
        }
        Flow::Continue
    }

    /// Records that the user did something: startup positioning stops
    /// and the one-shot status clears.
    fn act(&mut self) {
        self.view.end_startup();
        self.status = None;
        self.sync_view();
    }

    fn view_key(&mut self, c: Command) -> Flow {
        let (v, e) = (&mut self.view, c.extend);
        match c.action {
            Action::Up => v.go(Motion::Up, e),
            Action::Down => v.go(Motion::Down, e),
            Action::Left => v.go(Motion::Left, e),
            Action::Right => v.go(Motion::Right, e),
            Action::Home => v.go(Motion::LineStart, e),
            Action::End => v.go(Motion::LineEnd, e),
            Action::PageUp => v.go(Motion::PageUp, e),
            Action::PageDown => v.go(Motion::PageDown, e),
            Action::HalfPageUp => v.go(Motion::HalfPageUp, e),
            Action::HalfPageDown => v.go(Motion::HalfPageDown, e),
            Action::WordLeft => v.go(Motion::WordLeft, e),
            Action::WordRight => v.go(Motion::WordRight, e),
            Action::First => v.go(Motion::First, e),
            Action::Last => v.go(Motion::Last, e),
            Action::ScreenTop => v.go(Motion::ScreenTop, e),
            Action::ScreenMiddle => v.go(Motion::ScreenMiddle, e),
            Action::ScreenBottom => v.go(Motion::ScreenBottom, e),
            Action::PrevFile => v.go(Motion::PrevInput, e),
            Action::NextFile => v.go(Motion::NextInput, e),
            Action::SelectAll => v.select_all(),
            Action::ClearSelection => v.clear_selection(),
            Action::SetMark => v.set_mark(),
            Action::Copy => {
                self.copy();
            }
            Action::CopyAndQuit => {
                if v.selection().is_some() {
                    return if self.copy_and_wait() {
                        Flow::Quit
                    } else {
                        Flow::Continue
                    };
                }
                v.go(Motion::Down, false);
            }
            Action::Search | Action::SearchBack => self.open_search(c.action == Action::SearchBack),
            Action::SearchNext => {
                let found = v.find_next();
                self.report(found);
            }
            Action::SearchPrev => {
                let found = v.find_prev();
                self.report(found);
            }
            Action::CommandPrompt => self.open_command(),
            Action::Cycle => self.cycle(c.arg.as_deref().unwrap_or_default()),
            Action::Help => self.show_help(),
            Action::Reload => self.reload(true),
            Action::Quit => return Flow::Quit,
        }
        Flow::Continue
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
