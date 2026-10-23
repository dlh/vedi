//! The : prompt's commands: a line number, a setting for the rest of
//! the run, or what a key does.

use std::mem;

use crate::app::text_view::Motion;
use crate::app::{App, Flow, Focus, Kind, status};
use crate::input::action::Command;
use crate::input::config::{self, Config};
use crate::input::setting::{self, Cycle, Settings};
use crate::text::ansi::ViewStyle;
use crate::text::buffer::Pos;
use crate::view::screen::Terminal;

/// What the : prompt runs itself; the config verbs follow.
const BUILTINS: &[&str] = &["goto", "cycle", "help", "reload", "quit", "write"];

/// What the : prompt takes.
pub(super) fn commands() -> impl Iterator<Item = &'static str> {
    BUILTINS.iter().copied().chain(setting::at_prompt())
}

pub(super) fn is_command(name: &str) -> bool {
    commands().any(|c| c == name)
}

impl<T: Terminal> App<T> {
    pub(super) fn open_command(&mut self) {
        self.focus(Focus::Prompting(Kind::Command));
        self.mouse.end_drag();
        self.command.open_completing(':', &self.settings);
    }

    /// Runs a : line; an error is what it says.
    pub(super) fn run_command(&mut self, line: &str) -> Flow {
        self.command(line).unwrap_or_else(Flow::Say)
    }

    /// A number alone, or goto and a number, goes to that 1-based line,
    /// clamped to the input; cycle steps a setting; help, reload and
    /// quit do what their keys do; write saves the input to the file
    /// named by the rest of the line; a config verb sets what it would
    /// in a file, for the rest of the run. Nothing typed does nothing.
    fn command(&mut self, line: &str) -> Result<Flow, String> {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f[..] {
            [] => {}
            [n] | ["goto", n] if is_number(n) => self.goto(n),
            ["goto", ..] => return Err("goto takes a line number".into()),
            [verb @ ("help" | "reload" | "quit"), _, ..] => {
                return Err(format!("{verb} takes no argument"));
            }
            ["cycle" | "help" | "reload" | "quit", ..] => {
                let c: Command = line.parse().map_err(|e| format!("{e}"))?;
                return Ok(self.view_key(c));
            }
            ["write" | "write!"] => return Err("write takes a file name".into()),
            [verb @ ("write" | "write!"), ..] => {
                let name = line.trim_start()[verb.len()..].trim();
                return self.write(name, verb == "write!");
            }
            [verb, ..] if is_command(verb) => self.set(line)?,
            [verb, ..] => return Err(format!("unknown command {verb:?}")),
        }
        Ok(Flow::Continue)
    }

    fn goto(&mut self, n: &str) {
        // Past usize is past the end all the same.
        let n = n.parse::<usize>().unwrap_or(usize::MAX);
        let v = &mut self.view;
        let (_, lines) = v.buf().part(v.cursor().line);
        let line = lines
            .start
            .saturating_add(n)
            .saturating_sub(1)
            .min(lines.end.saturating_sub(1))
            .max(lines.start);
        v.go(Motion::To(Pos { line, col: 0 }), false);
    }

    /// Sets what a config line would.
    fn set(&mut self, line: &str) -> Result<(), String> {
        let mut c = Config {
            settings: self.settings.clone(),
            ..Config::default()
        };
        config::parse_line(line, &mut c)?;
        if !c.keys.is_empty() {
            self.keys = self.keys.apply(&c.keys);
        }
        self.apply_settings(c.settings);
        Ok(())
    }

    /// Puts `new` in force: the view is laid out for it, the text is
    /// decoded as its view style has it, and auto_reload turning off
    /// drops the reload a change asked for during the current one.
    fn apply_settings(&mut self, new: Settings) {
        let old = mem::replace(&mut self.settings, new);
        self.sync_view();
        if self.settings.view_style != old.view_style {
            let raw = self.settings.view_style == ViewStyle::Raw;
            self.text_view_mut().set_literal(raw);
        }
        if old.auto_reload && !self.settings.auto_reload {
            self.dirty = false;
        }
    }

    /// Sets a setting to the value after its current one, the first
    /// after the last, and says the new value unless the status line
    /// shows it anyway.
    pub(super) fn cycle(&mut self, name: &str) -> Flow {
        let Some(s) = setting::verb(name) else {
            return Flow::Continue;
        };
        match s.cycle {
            Cycle::No => Flow::Continue,
            Cycle::Toggle => self.toggle(name),
            Cycle::Values(_) => {
                let line = format!("{name} {}", s.next(&self.settings.value(name)));
                match self.run_command(&line) {
                    Flow::Continue if !status::summarizes(name) => Flow::Say(line),
                    flow => flow,
                }
            }
        }
    }

    /// Switches a toggle verb off, then on again.
    fn toggle(&mut self, name: &str) -> Flow {
        if name == "open_cmd" {
            return self.toggle_open_cmd();
        }
        Flow::Continue
    }

    /// Reads the files through open_cmd, or as they are. The selection
    /// goes: the lines it covered are not the new text's. A reload
    /// still being read is dropped: it may be waiting on a command
    /// that never ends.
    fn toggle_open_cmd(&mut self) -> Flow {
        if self.open.is_none() {
            return Flow::Say("not a file".into());
        }
        self.through_open_cmd = !self.through_open_cmd;
        self.view.deselect();
        self.next = None;
        self.dirty = false;
        self.reload_then(true, true)
    }

    /// The command the cursor's input is read through, or '' for none,
    /// as the status line reports it; the text may be set aside under
    /// help.
    pub(super) fn open_cmd_status(&self) -> String {
        let v = self.text_view();
        let (part, _) = v.buf().part(v.cursor().line);
        let argv = match &self.open_cmd {
            Some(f) if self.through_open_cmd => self.names.get(part).and_then(|n| f(n)),
            _ => None,
        };
        let cmd = argv.map_or_else(|| "''".to_string(), |a| a.join(" "));
        format!("open_cmd {cmd}")
    }
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}
