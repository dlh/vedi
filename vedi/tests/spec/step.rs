//! Prints a scenario as it runs, for `bin/spec-step`: each section,
//! then the screen it left if that changed, in a frame the screen's
//! size. It waits for Enter before each action and clears the terminal
//! for it, so the frame changes in place; with stdin not a terminal it
//! prints straight through, a rule between the steps.

use std::io::{self, BufRead, IsTerminal, Write};

use vedi::view::screen::{Terminal, TestTerminal};
use vedi::view::style::{Attrs, CUR_MATCH, MATCH};

use crate::spec::archive::Archive;
use crate::spec::scenario::{ACTIONS, DRIVES};

pub struct Stepper<'a> {
    a: &'a Archive,
    /// The section running.
    i: usize,
    /// The last frame printed.
    shown: String,
    /// And the terminal was cleared since.
    gone: bool,
    /// stdin is not a terminal or has ended, or q was typed.
    done: bool,
}

impl<'a> Stepper<'a> {
    pub fn new(a: &'a Archive) -> Stepper<'a> {
        let mut p = Stepper {
            a,
            i: 0,
            shown: String::new(),
            gone: false,
            // A pipe left open would never answer.
            done: !io::stdin().is_terminal(),
        };
        p.page();
        p
    }

    /// Starts a step with the prose, on a cleared terminal when the
    /// steps wait.
    fn page(&mut self) {
        if !self.done {
            print!("\x1b[H\x1b[2J");
            self.gone = true;
        }
        println!("\n{}", self.a.comment.trim());
    }

    /// Shows the first screen and waits, when the section that opened
    /// the pager goes on to change it.
    pub fn opened(&mut self, term: &TestTerminal) {
        if DRIVES.contains(&self.a.sections[self.i].name.as_str()) {
            println!();
            self.frame(term);
            self.wait();
        }
    }

    /// Prints the section that just ran and what it did, and waits when
    /// an action comes next.
    pub fn after(
        &mut self,
        term: Option<&TestTerminal>,
        started: bool,
        quit: bool,
        err: Option<&str>,
    ) {
        let sec = &self.a.sections[self.i];
        self.i += 1;
        println!("\n-- {} --", sec.name);
        if sec.name != "screen" {
            print!("{}", clip(&sec.body, 10));
        }
        let changed = term.is_some_and(|t| self.frame(t));
        let action = ACTIONS.contains(&sec.verb());
        if err.is_none() {
            if !action && started {
                println!("ok");
            } else if !changed && term.is_some() {
                println!("(screen unchanged)");
            }
        }
        if quit && action {
            println!("(the app quit)");
        }
        if err.is_none()
            && started
            && self.i < self.a.sections.len()
            && ACTIONS.contains(&self.a.sections[self.i].verb())
        {
            self.wait();
        }
    }

    /// Prints the screen and the cursor's place, unless they are as last
    /// printed and still on the terminal, and reports whether they
    /// changed.
    fn frame(&mut self, term: &TestTerminal) -> bool {
        let f = frame(term);
        let changed = f != self.shown;
        if changed || self.gone {
            print!("{f}");
        }
        self.shown = f;
        self.gone = false;
        changed
    }

    /// Ends a step. It reads a line: Enter clears the terminal for the
    /// next step, under the prose again; q stops waiting. Once nothing
    /// waits, a rule ends the step.
    fn wait(&mut self) {
        if !self.done {
            print!("\n[Enter] next, [q] run to the end ");
            let _ = io::stdout().flush();
            let mut line = String::new();
            let ended = !matches!(io::stdin().lock().read_line(&mut line), Ok(n) if n > 0);
            if ended {
                println!();
            }
            self.done = ended || line.trim() == "q";
            if !self.done {
                self.page();
                return;
            }
        }
        println!("\n{}", "━".repeat(42));
    }
}

/// Draws the screen in a box, with reverse video and search matches as
/// a terminal shows them and the cursor's cell underlined.
pub fn frame(term: &TestTerminal) -> String {
    let (w, h) = term.size();
    let cursor = term.cursor();
    let mut s = String::new();
    s.push_str(&format!("┌{}┐\n", "─".repeat(w)));
    for y in 0..h {
        s.push('│');
        let mut x = 0;
        while x < w {
            let c = term.cell(x, y);
            let mut sgr: Vec<&str> = Vec::new();
            if c.style.attrs.contains(Attrs::REVERSE) {
                sgr.push("7");
            }
            if c.style == MATCH {
                sgr.extend(["30", "103"]);
            }
            if c.style == CUR_MATCH {
                sgr.extend(["97", "45"]);
            }
            if cursor == Some((x, y)) {
                sgr.push("4");
            }
            if sgr.is_empty() {
                s.push_str(&c.text);
            } else {
                s.push_str(&format!("\x1b[{}m{}\x1b[m", sgr.join(";"), c.text));
            }
            x += usize::from(c.width.max(1));
        }
        s.push_str("│\n");
    }
    s.push_str(&format!("└{}┘\n", "─".repeat(w)));
    match cursor {
        Some((x, y)) => s.push_str(&format!("cursor {y} {x}\n")),
        None => s.push_str("cursor hidden\n"),
    }
    s
}

/// `body` cut to `n` lines, saying how many it left out.
fn clip(body: &str, n: usize) -> String {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    if lines.len() <= n {
        return body.to_string();
    }
    let left = body.matches('\n').count() - n;
    format!("{}… {left} more lines\n", lines[..n].concat())
}
