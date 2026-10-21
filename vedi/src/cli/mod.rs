//! The command line, $VEDI and the shell completion scripts.

mod completion;
mod parse;

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::app::{self, Copier, Done, STDIN, Sink, StartScreen};
use crate::input::config::{self, Config};
use crate::input::setting::Settings;
use crate::sys::opencmd::Killer;
use crate::sys::{save, words};

pub use completion::completion;
pub use parse::parse;

pub const USAGE: &str = r#"usage: vedi [flags] [file...]

  -S, --[no-]wrap              wrap long lines; -S is --no-wrap (default: on)
  --wrap-style STYLE          wrap at the screen's edge (char) or at words (word)
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
  --open-cmd CMD               read each file by running CMD, %s the file
  --no-open-cmd                read each file as it is
  --tab-width N                draw a tab as N cells (default: 8)
  --[no-]edge-markers          mark text off the sides with < and >, a wrapped row with \ (default: off)
  --[no-]file-separators       with several inputs, draw a row naming each (default: on)
  --[no-]status-line           draw the status line; off, only for a prompt or a message (default: on)
  --view-style STYLE           draw the text in color, without (plain) or with its escapes as text (raw)
  --config FILE                read the config from FILE, not ~/.config/vedi/vedi.conf
  --completion SHELL           print the completion script for bash, zsh or fish
  -h, --help                   show this help
  -v, --version                print the version

Flags in the VEDI environment variable apply to every run; the
command line wins.

Keys: arrows move, Shift+arrows select, Ctrl+C/y copy, Enter copy and
quit, / search, n/N next/prev, w toggle wrap, q quit.

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
    /// and no files are stdin. The copier does nothing, the clock is
    /// `Instant::now` and nothing reopens the input, for `main` to
    /// replace.
    pub fn app(&self, files: &[OsString], cfg: &Config, macos: bool) -> app::Options {
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
            copier: Box::new(NoCopier),
            wake: Arc::new(|| {}),
            clock: Box::new(Instant::now),
            macos,
            keys: cfg.keymap(macos),
            open: None,
            open_cmd: None,
            write: Box::new(|name, overwrite| {
                save::open(name, overwrite).map(|s| Box::new(s) as Box<dyn Sink>)
            }),
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

    /// The command each file is read through: the flag's for every
    /// file, else the config's by name; `None` when no file has one.
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
                    cfg.open_cmd_for(name).map(<[String]>::to_vec)
                })));
            }
            CmdFlag::Off => return Ok(None),
            CmdFlag::Cmd(flag) => flag,
        };
        let argv = words::split(flag).map_err(|e| format!("open command: {e}"))?;
        if argv.is_empty() {
            return Ok(None);
        }
        Ok(Some(Box::new(move |_| Some(argv.clone()))))
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

struct NoCopier;

impl Copier for NoCopier {
    fn copy(&mut self, _: &str, done: Done) -> Option<Killer> {
        done(Ok(()));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::config::OpenCmd;
    use crate::input::setting;
    use crate::text::ansi::ViewStyle;
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

    /// The options with `src` as the setting flags' lines.
    fn flags(src: &str) -> Options {
        Options {
            lines: src.lines().map(String::from).collect(),
            ..none()
        }
    }

    /// The flag decides; without one the config does; without either a
    /// changed file is read again.
    #[test]
    fn reloads() {
        for (f, c, want) in [
            ("", "", true),
            ("", "auto_reload no", false),
            ("auto_reload no", "", false),
            ("auto_reload yes", "auto_reload no", true),
        ] {
            assert_eq!(flags(f).settings(&cfg(c)).auto_reload, want, "{f:?} {c:?}");
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
        let got = o.app(&[], &cfg(""), false);
        assert_eq!(got.settings.mode, Mode::NoWrap);
        assert_eq!(got.start_line, Some(3));
        assert_eq!(got.screen, Some(screen));
        assert!(got.open.is_none());
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
            let got = flags(f).app(&[], &cfg(c), false);
            assert_eq!(
                (got.settings.mode, got.settings.wrap_style),
                (mode, want),
                "{f:?} {c:?}"
            );
        }
    }

    /// The flag decides the rows -F leaves below the text; without one
    /// the config does; without either it is one.
    #[test]
    fn rows_below() {
        for (f, c, want) in [
            ("", "", 1),
            ("", "one_page_rows_below 3", 3),
            ("one_page_rows_below 2", "one_page_rows_below 3", 2),
            ("one_page_rows_below 2", "", 2),
        ] {
            assert_eq!(
                flags(f).settings(&cfg(c)).one_page_rows_below,
                want,
                "{f:?} {c:?}"
            );
        }
    }

    /// The flag decides the tab width; without one the config does;
    /// without either it is zero, the app's default.
    #[test]
    fn app_tab_width() {
        for (f, c, want) in [
            ("", "", 0),
            ("", "tab_width 4", 4),
            ("tab_width 2", "tab_width 4", 2),
            ("tab_width 2", "", 2),
        ] {
            assert_eq!(flags(f).settings(&cfg(c)).tab_width, want, "{f:?} {c:?}");
        }
    }

    /// The flag decides the open command for every file, --no-open-cmd
    /// included; without one the config does, by name; without either
    /// there is none. A flag that cannot be split is an error.
    #[test]
    fn open_cmd() {
        let c = Config {
            open_cmds: vec![OpenCmd {
                pattern: Some(glob::Pattern::new("*.md").unwrap()),
                argv: vec!["mdcat".into(), "%s".into()],
            }],
            ..Config::default()
        };
        let cmd = |s: &str| CmdFlag::Cmd(s.to_string());
        for (flag, c, want) in [
            (CmdFlag::Unset, cfg(""), None),
            (CmdFlag::Unset, c.clone(), Some(vec!["mdcat", "%s"])),
            (cmd("bat %s"), c.clone(), Some(vec!["bat", "%s"])),
            (cmd("bat '%s'"), cfg(""), Some(vec!["bat", "%s"])),
            (cmd(""), c.clone(), None),
            (CmdFlag::Off, c.clone(), None),
        ] {
            let o = Options {
                open_cmd: flag.clone(),
                ..none()
            };
            let open = o.open_cmd(&c).unwrap();
            let got = open.and_then(|f| f("a.md"));
            let want = want.map(|w| w.into_iter().map(String::from).collect::<Vec<_>>());
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

    /// --edge-markers and --no-edge-markers decide; without one the
    /// config does; without either the markers are off.
    #[test]
    fn edge_markers() {
        for (f, c, want) in [
            ("", "", false),
            ("", "edge_markers yes", true),
            ("edge_markers yes", "", true),
            ("edge_markers no", "edge_markers yes", false),
        ] {
            assert_eq!(flags(f).settings(&cfg(c)).edge_markers, want, "{f:?} {c:?}");
        }
    }

    /// --file-separators and --no-file-separators decide; without one
    /// the config does; without either the rows are drawn.
    #[test]
    fn file_separators() {
        for (f, c, want) in [
            ("", "", true),
            ("", "file_separators no", false),
            ("file_separators yes", "file_separators no", true),
            ("file_separators no", "", false),
        ] {
            assert_eq!(
                flags(f).settings(&cfg(c)).file_separators,
                want,
                "{f:?} {c:?}"
            );
        }
    }

    /// --status-line and --no-status-line decide; without one the
    /// config does; without either the line is drawn.
    #[test]
    fn status_line() {
        for (f, c, hide) in [
            ("", "", false),
            ("", "status_line no", true),
            ("status_line yes", "status_line no", false),
            ("status_line no", "", true),
        ] {
            assert_eq!(!flags(f).settings(&cfg(c)).status_line, hide, "{f:?} {c:?}");
        }
    }

    /// --view-style decides; without it the config does.
    #[test]
    fn app_view_style() {
        for (f, c, want) in [
            ("", "", ViewStyle::Color),
            ("", "view_style plain", ViewStyle::Plain),
            ("view_style color", "view_style plain", ViewStyle::Color),
            ("view_style plain", "", ViewStyle::Plain),
        ] {
            assert_eq!(flags(f).settings(&cfg(c)).view_style, want, "{f:?} {c:?}");
        }
    }

    /// Each file is named as given; - and no files are stdin.
    #[test]
    fn app_names() {
        let names = |files: &[&str]| {
            let files: Vec<OsString> = files.iter().map(OsString::from).collect();
            none().app(&files, &cfg(""), false).names
        };
        assert_eq!(names(&[]), [STDIN]);
        assert_eq!(names(&["a", "-", "b"]), ["a", STDIN, "b"]);
    }
}
