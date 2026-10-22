//! The : prompt's commands: a line number, a setting for the rest of
//! the run, or what a key does.

use crate::app::text_view::Motion;
use crate::app::{App, Flow, Focus, Kind};
use crate::input::action::Command;
use crate::input::config::{self, Config};
use crate::input::setting::{self, Cycle};
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

    /// Runs a : line, reporting an error on the status line.
    pub(super) fn run_command(&mut self, line: &str) -> Flow {
        match self.command(line) {
            Ok(flow) => flow,
            Err(e) => {
                self.status = Some(e);
                Flow::Continue
            }
        }
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
                self.write(name, verb == "write!")?;
            }
            [verb, ..] if is_command(verb) => self.set(verb, line)?,
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
    fn set(&mut self, verb: &str, line: &str) -> Result<(), String> {
        let mut c = Config {
            settings: self.settings.clone(),
            ..Config::default()
        };
        config::parse_line(line, &mut c)?;
        self.settings = c.settings;
        self.sync_view();
        match verb {
            // A change during a reload asked for another.
            "auto_reload" if !self.settings.auto_reload => self.dirty = false,
            "view_style" => self.set_literal(),
            "map" => self.keys = self.keys.apply(&c.keys),
            _ => {}
        }
        Ok(())
    }

    /// Sets a setting to the value after its current one, the first
    /// after the last, and reports the new value on the status line
    /// unless the line shows it anyway.
    pub(super) fn cycle(&mut self, name: &str) {
        let Some(s) = setting::verb(name) else {
            return;
        };
        match s.cycle {
            Cycle::No => {}
            Cycle::Toggle => self.toggle(name),
            Cycle::Values(_) => {
                let line = format!("{name} {}", s.next(&self.settings.value(name)));
                self.run_command(&line);
                if !s.shown {
                    self.status = Some(line);
                }
            }
        }
    }

    /// Switches a toggle verb off, then on again.
    fn toggle(&mut self, name: &str) {
        if name == "open_cmd" {
            self.toggle_open_cmd();
        }
    }

    /// Reads the files through open_cmd, or as they are. The selection
    /// goes: the lines it covered are not the new text's. A reload
    /// still being read is dropped: it may be waiting on a command
    /// that never ends.
    fn toggle_open_cmd(&mut self) {
        if self.open.is_none() {
            self.status = Some("not a file".into());
            return;
        }
        self.through_open_cmd = !self.through_open_cmd;
        self.view.deselect();
        self.next = None;
        self.dirty = false;
        self.reload_then(true, true);
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

    /// Decodes the text as the view style has it.
    fn set_literal(&mut self) {
        let raw = self.settings.view_style == ViewStyle::Raw;
        self.text_view_mut().set_literal(raw);
    }
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}
