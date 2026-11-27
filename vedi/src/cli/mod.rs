//! The command line, $VEDI and the shell completion scripts.

mod completion;
mod parse;

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use crate::app::{self, Entry, STDIN, Sink, StartScreen};
use crate::input::config::{self, Config};
use crate::input::setting::Settings;
use crate::sys::{save, words};

pub use completion::completion;
pub use parse::parse;

pub const USAGE: &str = r#"usage: vedi [flags] [file...]

  -S, --[no-]wrap              wrap long lines; -S is --no-wrap (default: on)
  --wrap-style STYLE           wrap at the screen's edge (char) or at words (word)
  --wrap-width N               wrap rows at N cells when the screen is wider (default: off)
  -F, --[no-]quit-if-one-page  print the text and quit if it fits the screen (default: off)
  --one-page-rows-below N      rows -F leaves below the text, for the prompt (default: 1)
  --[no-]auto-reload           read a file again when it changes on disk (default: on)
  +G                           start at the last line and follow until EOF
  +N                           start with line N at the top
  --scrolled-by N              start on the last screenful, scrolled N rows up
  --cursor-row N               put the cursor on row N of the last screenful
  --cursor-col N               put the cursor in column N of the last screenful
  --clipboard-cmd CMD          pipe copied text to CMD instead of OSC 52
  --no-clipboard-cmd           copy with OSC 52, or pbcopy in Terminal.app
  --[no-]copy-on-select        copy what the mouse selects when the button comes up (default: on)
  --open-cmd CMD               read each file by running CMD, %s the file
  --no-open-cmd                read each file as it is
  --tab-width N                draw a tab as N cells (default: 8)
  --[no-]edge-markers          mark text off the sides with < and >, a wrapped row with \ (default: off)
  --[no-]file-separators       with several inputs, draw a row naming each (default: on)
  --[no-]end-markers          start each row below the text with ~ (default: on)
  --[no-]status-line           draw the status line; off, only for a prompt or a message (default: on)
  --view-style STYLE           draw the text in color, without (plain) or with its escapes as text (raw)
  --hint-order ORDER           number hints from the top (top_down) or the bottom (bottom_up)
  --[no-]line-numbers          number the lines (default: off)
  --line-numbers-side SIDE     number them at the left or right edge of the screen (default: left)
  --config FILE                read the config from FILE, not ~/.config/vedi/vedi.conf
  --completion SHELL           print the completion script for bash, zsh or fish
  -h, --help                   show this help
  -v, --version                print the version

Flags in the VEDI environment variable apply to every run; the
command line wins.

Keys: arrows move, Shift+arrows select, Ctrl+C/y copy, / search, n/N
next/prev, w toggle wrap, q quit.

Docs: https://github.com/dlh/vedi
"#;

/// The flags.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Options {
    /// The setting flags as config lines, in order, laid over the
    /// file's.
    pub lines: Vec<String>,
    pub quit_if_one_page: bool,
    /// 1-based.
    pub start_line: Option<usize>,
    pub follow: bool,
    /// Set by any of --scrolled-by, --cursor-row, --cursor-col.
    pub screen: Option<StartScreen>,
    pub clipboard_cmd: CmdFlag,
    pub open_cmd: CmdFlag,
    /// `None` for the default location.
    pub config: Option<PathBuf>,
    /// The shell to print the completion script for.
    pub completion: Option<String>,
    pub help: bool,
    pub version: bool,
}

/// --X-cmd CMD, --no-X-cmd, or neither, leaving it to the config.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum CmdFlag {
    #[default]
    Unset,
    Off,
    Cmd(String),
}

pub use crate::app::OpenCmdFor;

impl Options {
    /// The app's options: each setting the flag's, else the config's,
    /// else the default. Inputs are named after `files` as given; `-`
    /// and no files are stdin.
    pub fn app(&self, files: &[OsString], cfg: &Config, runtime: app::Runtime) -> app::Options {
        let names = if files.is_empty() {
            vec![STDIN.to_string()]
        } else {
            files
                .iter()
                .map(|f| {
                    if f == "-" {
                        STDIN.to_string()
                    } else {
                        f.to_string_lossy().into_owned()
                    }
                })
                .collect()
        };
        app::Options {
            names,
            settings: self.settings(cfg),
            start_line: self.start_line,
            follow: self.follow,
            screen: self.screen,
            clock: Box::new(Instant::now),
            keys: cfg.keymap(),
            runtime,
            write: Box::new(|name, overwrite| {
                save::open(name, overwrite).map(|s| Box::new(s) as Box<dyn Sink>)
            }),
            list: Rc::new(|dir| {
                let mut entries = Vec::new();
                for e in fs::read_dir(dir)? {
                    let e = e?;
                    // The prompt holds chars: a name that is not UTF-8
                    // cannot be typed as it is.
                    let Ok(name) = e.file_name().into_string() else {
                        continue;
                    };
                    // A link to a directory is one to go into.
                    let dir = fs::metadata(e.path()).is_ok_and(|m| m.is_dir());
                    entries.push(Entry { name, dir });
                }
                Ok(entries)
            }),
            home: std::env::var_os("HOME")
                .filter(|h| !h.is_empty())
                .map(PathBuf::from),
        }
    }

    /// The settings: the flag lines laid over the file's.
    pub fn settings(&self, cfg: &Config) -> Settings {
        let mut c = Config {
            settings: cfg.settings.clone(),
            ..Config::default()
        };
        for line in &self.lines {
            config::parse_line(line, &mut c).expect("checked as the flag was parsed");
        }
        c.settings
    }

    /// The commands each file can be read through: the flag's for
    /// every file, else the config's by name; `None` when no file has
    /// one.
    /// The error is a flag that does not split.
    pub fn open_cmd(&self, cfg: &Config) -> Result<Option<OpenCmdFor>, String> {
        let flag = match &self.open_cmd {
            CmdFlag::Unset if cfg.open_cmds.is_empty() => return Ok(None),
            CmdFlag::Unset => {
                let cfg = Config {
                    open_cmds: cfg.open_cmds.clone(),
                    ..Config::default()
                };
                return Ok(Some(Box::new(move |name| {
                    cfg.open_cmds_for(name)
                        .into_iter()
                        .map(<[String]>::to_vec)
                        .collect()
                })));
            }
            CmdFlag::Off => return Ok(None),
            CmdFlag::Cmd(flag) => flag,
        };
        let argv = words::split(flag).map_err(|e| format!("open command: {e}"))?;
        if argv.is_empty() {
            return Ok(None);
        }
        Ok(Some(Box::new(move |_| vec![argv.clone()])))
    }

    /// The command copied text is piped to; `None` for the terminal's
    /// clipboard.
    pub fn clipboard_cmd(&self, cfg: &Config) -> Option<String> {
        match &self.clipboard_cmd {
            CmdFlag::Unset => cfg.clipboard_cmd.clone(),
            CmdFlag::Off => None,
            CmdFlag::Cmd(flag) => Some(flag.clone()),
        }
        .filter(|c| !c.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app::{Copier, Done};
    use crate::input::config::{Names, OpenCmd};
    use crate::input::setting;
    use crate::sys::proc::Killer;
    use crate::view::layout::{Mode, WrapStyle};

    /// The usage text names each flag a setting has.
    #[test]
    fn usage_names_flag_verbs() {
        for v in setting::VERBS.iter().filter(|v| v.flag) {
            let flag = v.name.replace('_', "-");
            let want = if v.yes_no() {
                format!("--[no-]{flag} ")
            } else {
                format!("--{flag} ")
            };
            assert!(USAGE.contains(&want), "{want:?} is not in USAGE");
        }
    }

    fn none() -> Options {
        Options::default()
    }

    fn cfg(src: &str) -> Config {
        config::parse("vedi.conf", src).unwrap()
    }

    fn runtime() -> app::Runtime {
        struct NoCopier;
        impl Copier for NoCopier {
            fn copy(&mut self, _: &str, done: Done) -> Option<Killer> {
                done(Ok(()));
                None
            }
        }
        app::Runtime {
            copier: Box::new(NoCopier),
            wake: Arc::new(|| {}),
            open: None,
            open_cmd: None,
        }
    }

    /// The options with `src` as the setting flags' lines.
    fn flags(src: &str) -> Options {
        Options {
            lines: src.lines().map(String::from).collect(),
            ..none()
        }
    }

    /// For each setting a flag sets, the flag decides; without one the
    /// config does; without either the default.
    #[test]
    fn flag_beats_config() {
        for v in setting::VERBS.iter().filter(|v| v.flag) {
            let default = v.value(&Settings::default());
            let values = match v.values() {
                [] => &["2", "3"],
                vs => vs,
            };
            // a differs from the default so the config is seen to apply,
            // b from a so the flag is seen to win.
            let a = *values.iter().find(|&&x| x != default).unwrap();
            let b = *values.iter().find(|&&x| x != a).unwrap();
            let line = |x: &str| format!("{} {x}", v.name);
            for (f, c, want) in [
                (String::new(), String::new(), default.as_str()),
                (String::new(), line(a), a),
                (line(b), String::new(), b),
                (line(b), line(a), b),
            ] {
                let got = v.value(&flags(&f).settings(&cfg(&c)));
                assert_eq!(got, want, "{f:?} {c:?}");
            }
        }
    }

    #[test]
    fn app() {
        let screen = StartScreen {
            cursor_row: 2,
            ..StartScreen::default()
        };
        let o = Options {
            start_line: Some(3),
            screen: Some(screen),
            ..flags("wrap no")
        };
        let got = o.app(&[], &cfg(""), runtime());
        assert_eq!(got.settings.mode, Mode::NoWrap);
        assert_eq!(got.start_line, Some(3));
        assert_eq!(got.screen, Some(screen));
    }

    /// The flag decides the wrap mode and clipboard command; without one
    /// the config does.
    #[test]
    fn app_config() {
        let c = cfg("wrap no\nclipboard_cmd wl-copy\n");
        let cmd = |s: &str| CmdFlag::Cmd(s.to_string());
        for (o, c, mode, want) in [
            (none(), cfg(""), Mode::Wrap, None),
            (none(), c.clone(), Mode::NoWrap, Some("wl-copy")),
            (
                Options {
                    clipboard_cmd: cmd("pbcopy"),
                    ..flags("wrap yes")
                },
                c.clone(),
                Mode::Wrap,
                Some("pbcopy"),
            ),
            (
                Options {
                    clipboard_cmd: CmdFlag::Off,
                    ..none()
                },
                c.clone(),
                Mode::NoWrap,
                None,
            ),
            (
                Options {
                    clipboard_cmd: cmd(""),
                    ..none()
                },
                c.clone(),
                Mode::NoWrap,
                None,
            ),
            (flags("wrap no"), cfg(""), Mode::NoWrap, None),
        ] {
            assert_eq!(o.settings(&c).mode, mode, "{o:?} {c:?}");
            assert_eq!(o.clipboard_cmd(&c).as_deref(), want, "{o:?} {c:?}");
        }
    }

    /// The flag decides where wrap breaks rows; without one the config
    /// does.
    #[test]
    fn app_wrap_style() {
        for (f, c, mode, want) in [
            ("", "", Mode::Wrap, WrapStyle::Char),
            ("", "wrap_style word", Mode::Wrap, WrapStyle::Word),
            ("wrap_style word", "wrap no", Mode::NoWrap, WrapStyle::Word),
            (
                "wrap_style char",
                "wrap_style word",
                Mode::Wrap,
                WrapStyle::Char,
            ),
            ("wrap yes", "wrap_style word", Mode::Wrap, WrapStyle::Word),
            ("wrap no", "wrap_style word", Mode::NoWrap, WrapStyle::Word),
        ] {
            let got = flags(f).app(&[], &cfg(c), runtime());
            assert_eq!(
                (got.settings.mode, got.settings.wrap_style),
                (mode, want),
                "{f:?} {c:?}"
            );
        }
    }

    /// The flag decides the one open command for every file,
    /// --no-open-cmd included; without one the config does, by name,
    /// each matching line; without either there is none. A flag that
    /// cannot be split is an error.
    #[test]
    fn open_cmd() {
        let c = Config {
            open_cmds: vec![
                OpenCmd {
                    names: Names::Rest,
                    argv: vec!["cat".into()],
                },
                OpenCmd {
                    names: Names::Matching(glob::Pattern::new("*.md").unwrap()),
                    argv: vec!["mdcat".into(), "%s".into()],
                },
            ],
            ..Config::default()
        };
        let cmd = |s: &str| CmdFlag::Cmd(s.to_string());
        for (flag, c, want) in [
            (CmdFlag::Unset, cfg(""), None),
            (
                CmdFlag::Unset,
                c.clone(),
                Some(vec![&["mdcat", "%s"][..], &["cat"]]),
            ),
            (cmd("bat %s"), c.clone(), Some(vec![&["bat", "%s"][..]])),
            (cmd("bat '%s'"), cfg(""), Some(vec![&["bat", "%s"][..]])),
            (cmd(""), c.clone(), None),
            (CmdFlag::Off, c.clone(), None),
        ] {
            let o = Options {
                open_cmd: flag.clone(),
                ..none()
            };
            let open = o.open_cmd(&c).unwrap();
            let got = open.map(|f| f("a.md"));
            let want = want.map(|w| {
                w.into_iter()
                    .map(|a| a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            });
            assert_eq!(got, want, "{flag:?} {c:?}");
        }
        let o = Options {
            open_cmd: cmd("bat '%s"),
            ..none()
        };
        assert_eq!(
            o.open_cmd(&c).err(),
            Some("open command: unclosed '".to_string())
        );
    }

    /// Each file is named as given; - and no files are stdin.
    #[test]
    fn app_names() {
        let names = |files: &[&str]| {
            let files: Vec<OsString> = files.iter().map(OsString::from).collect();
            none().app(&files, &cfg(""), runtime()).names
        };
        assert_eq!(names(&[]), [STDIN]);
        assert_eq!(names(&["a", "-", "b"]), ["a", STDIN, "b"]);
    }
}
