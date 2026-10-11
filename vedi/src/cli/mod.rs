//! The command line, $VEDI and the shell completion scripts.

mod completion;
mod parse;

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Instant;

use crate::app::{self, Copier, STDIN, StartScreen};
use crate::input::config::Config;
use crate::sys::words;
use crate::text::ansi::ViewStyle;
use crate::view::layout::{Mode, WrapStyle};

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

/// The flags; `None` leaves a setting to the config.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Options {
    pub wrap: Option<bool>,
    pub wrap_style: Option<WrapStyle>,
    pub quit_if_one_page: bool,
    pub one_page_rows_below: Option<usize>,
    /// 1-based.
    pub start_line: Option<usize>,
    pub follow: bool,
    /// Set by any of --scrolled-by, --cursor-row, --cursor-col.
    pub screen: Option<StartScreen>,
    /// `Some(None)` is --no-clipboard-cmd.
    pub clipboard_cmd: Option<Option<String>>,
    /// `Some(None)` is --no-open-cmd.
    pub open_cmd: Option<Option<String>>,
    pub tab_width: Option<usize>,
    /// `None` for the default location.
    pub config: Option<PathBuf>,
    pub auto_reload: Option<bool>,
    pub edge_markers: Option<bool>,
    pub file_separators: Option<bool>,
    pub status_line: Option<bool>,
    pub view_style: Option<ViewStyle>,
    /// The shell to print the completion script for.
    pub completion: Option<String>,
    pub help: bool,
    pub version: bool,
}

/// Reads a file through a command: its argv by file name, %s the
/// name; `None` to read it as it is.
pub type OpenCmdFor = Box<dyn Fn(&str) -> Option<Vec<String>>>;

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
            mode: match self.wrap.or(cfg.wrap) {
                Some(false) => Mode::NoWrap,
                _ => Mode::Wrap,
            },
            wrap_style: self.wrap_style.or(cfg.wrap_style).unwrap_or_default(),
            start_line: self.start_line,
            follow: self.follow,
            screen: self.screen,
            copier: Box::new(NoCopier),
            clock: Box::new(Instant::now),
            macos,
            keys: cfg.keymap(macos),
            tab_width: self.tab_width.or(cfg.tab_width).unwrap_or(0),
            edge_markers: self.edge_markers.or(cfg.edge_markers).unwrap_or(false),
            file_separators: self.separators(cfg),
            auto_reload: self.reloads(cfg),
            hide_status: !self.status_line.or(cfg.status_line).unwrap_or(true),
            view_style: self.view(cfg),
            open: None,
        }
    }

    /// How the text's styling is shown.
    pub fn view(&self, cfg: &Config) -> ViewStyle {
        self.view_style.or(cfg.view_style).unwrap_or_default()
    }

    /// Whether a row names each input, when there are several.
    pub fn separators(&self, cfg: &Config) -> bool {
        self.file_separators.or(cfg.file_separators).unwrap_or(true)
    }

    /// The rows -F leaves below the text.
    pub fn rows_below(&self, cfg: &Config) -> usize {
        self.one_page_rows_below
            .or(cfg.one_page_rows_below)
            .unwrap_or(1)
            .max(1)
    }

    /// Whether a file that changes on disk is read again.
    pub fn reloads(&self, cfg: &Config) -> bool {
        self.auto_reload.or(cfg.auto_reload).unwrap_or(true)
    }

    /// The command each file is read through: the flag's for every
    /// file, else the config's by name; `None` when no file has one.
    /// The error is a flag that does not split.
    pub fn open_cmd(&self, cfg: &Config) -> Result<Option<OpenCmdFor>, String> {
        let flag = match &self.open_cmd {
            None if cfg.open_cmds.is_empty() => return Ok(None),
            None => {
                let cfg = Config {
                    open_cmds: cfg.open_cmds.clone(),
                    ..Config::default()
                };
                return Ok(Some(Box::new(move |name| {
                    cfg.open_cmd_for(name).map(<[String]>::to_vec)
                })));
            }
            Some(None) => return Ok(None),
            Some(Some(flag)) => flag,
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
            Some(flag) => flag.clone(),
            None => cfg.clipboard_cmd.clone(),
        }
        .filter(|c| !c.is_empty())
    }
}

struct NoCopier;

impl Copier for NoCopier {
    fn copy(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::config::OpenCmd;

    fn none() -> Options {
        Options::default()
    }

    fn cfg() -> Config {
        Config::default()
    }

    /// The flag decides; without one the config does; without either a
    /// changed file is read again.
    #[test]
    fn reloads() {
        for (flag, c, want) in [
            (None, cfg(), true),
            (
                None,
                Config {
                    auto_reload: Some(false),
                    ..cfg()
                },
                false,
            ),
            (Some(false), cfg(), false),
            (
                Some(true),
                Config {
                    auto_reload: Some(false),
                    ..cfg()
                },
                true,
            ),
        ] {
            let o = Options {
                auto_reload: flag,
                ..none()
            };
            assert_eq!(o.reloads(&c), want, "{flag:?} {c:?}");
        }
    }

    #[test]
    fn app() {
        let screen = StartScreen {
            cursor_row: 2,
            ..StartScreen::default()
        };
        let o = Options {
            wrap: Some(false),
            start_line: Some(3),
            screen: Some(screen),
            ..none()
        };
        let got = o.app(&[], &cfg(), false);
        assert_eq!(got.mode, Mode::NoWrap);
        assert_eq!(got.start_line, Some(3));
        assert_eq!(got.screen, Some(screen));
        assert!(got.open.is_none());
    }

    /// The flag decides the wrap mode and clipboard command; without one
    /// the config does.
    #[test]
    fn app_config() {
        let c = Config {
            wrap: Some(false),
            clipboard_cmd: Some("wl-copy".into()),
            ..cfg()
        };
        let cmd = |s: &str| Some(Some(s.to_string()));
        for (o, c, mode, want) in [
            (none(), cfg(), Mode::Wrap, None),
            (none(), c.clone(), Mode::NoWrap, Some("wl-copy")),
            (
                Options {
                    wrap: Some(true),
                    clipboard_cmd: cmd("pbcopy"),
                    ..none()
                },
                c.clone(),
                Mode::Wrap,
                Some("pbcopy"),
            ),
            (
                Options {
                    clipboard_cmd: Some(None),
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
            (
                Options {
                    wrap: Some(false),
                    ..none()
                },
                cfg(),
                Mode::NoWrap,
                None,
            ),
        ] {
            assert_eq!(o.app(&[], &c, false).mode, mode, "{o:?} {c:?}");
            assert_eq!(o.clipboard_cmd(&c).as_deref(), want, "{o:?} {c:?}");
        }
    }

    /// The flag decides where wrap breaks rows; without one the config
    /// does.
    #[test]
    fn app_wrap_style() {
        let word = Config {
            wrap_style: Some(WrapStyle::Word),
            ..cfg()
        };
        for (o, c, mode, want) in [
            (none(), cfg(), Mode::Wrap, WrapStyle::Char),
            (none(), word.clone(), Mode::Wrap, WrapStyle::Word),
            (
                Options {
                    wrap_style: Some(WrapStyle::Word),
                    ..none()
                },
                Config {
                    wrap: Some(false),
                    ..cfg()
                },
                Mode::NoWrap,
                WrapStyle::Word,
            ),
            (
                Options {
                    wrap_style: Some(WrapStyle::Char),
                    ..none()
                },
                word.clone(),
                Mode::Wrap,
                WrapStyle::Char,
            ),
            (
                Options {
                    wrap: Some(true),
                    ..none()
                },
                word.clone(),
                Mode::Wrap,
                WrapStyle::Word,
            ),
            (
                Options {
                    wrap: Some(false),
                    ..none()
                },
                word.clone(),
                Mode::NoWrap,
                WrapStyle::Word,
            ),
        ] {
            let got = o.app(&[], &c, false);
            assert_eq!((got.mode, got.wrap_style), (mode, want), "{o:?} {c:?}");
        }
    }

    /// The flag decides the rows -F leaves below the text; without one
    /// the config does; without either it is one.
    #[test]
    fn rows_below() {
        for (flag, c, want) in [
            (None, cfg(), 1),
            (
                None,
                Config {
                    one_page_rows_below: Some(3),
                    ..cfg()
                },
                3,
            ),
            (
                Some(2),
                Config {
                    one_page_rows_below: Some(3),
                    ..cfg()
                },
                2,
            ),
            (Some(2), cfg(), 2),
        ] {
            let o = Options {
                one_page_rows_below: flag,
                ..none()
            };
            assert_eq!(o.rows_below(&c), want, "{flag:?} {c:?}");
        }
    }

    /// The flag decides the tab width; without one the config does;
    /// without either it is zero, the app's default.
    #[test]
    fn app_tab_width() {
        for (flag, c, want) in [
            (None, cfg(), 0),
            (
                None,
                Config {
                    tab_width: Some(4),
                    ..cfg()
                },
                4,
            ),
            (
                Some(2),
                Config {
                    tab_width: Some(4),
                    ..cfg()
                },
                2,
            ),
            (Some(2), cfg(), 2),
        ] {
            let o = Options {
                tab_width: flag,
                ..none()
            };
            assert_eq!(o.app(&[], &c, false).tab_width, want, "{flag:?} {c:?}");
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
            ..cfg()
        };
        let cmd = |s: &str| Some(Some(s.to_string()));
        for (flag, c, want) in [
            (None, cfg(), None),
            (None, c.clone(), Some(vec!["mdcat", "%s"])),
            (cmd("bat %s"), c.clone(), Some(vec!["bat", "%s"])),
            (cmd("bat '%s'"), cfg(), Some(vec!["bat", "%s"])),
            (cmd(""), c.clone(), None),
            (Some(None), c.clone(), None),
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
        for (flag, c, want) in [
            (None, cfg(), false),
            (
                None,
                Config {
                    edge_markers: Some(true),
                    ..cfg()
                },
                true,
            ),
            (Some(true), cfg(), true),
            (
                Some(false),
                Config {
                    edge_markers: Some(true),
                    ..cfg()
                },
                false,
            ),
        ] {
            let o = Options {
                edge_markers: flag,
                ..none()
            };
            assert_eq!(o.app(&[], &c, false).edge_markers, want, "{flag:?} {c:?}");
        }
    }

    /// --file-separators and --no-file-separators decide; without one
    /// the config does; without either the rows are drawn.
    #[test]
    fn file_separators() {
        for (flag, c, want) in [
            (None, cfg(), true),
            (
                None,
                Config {
                    file_separators: Some(false),
                    ..cfg()
                },
                false,
            ),
            (
                Some(true),
                Config {
                    file_separators: Some(false),
                    ..cfg()
                },
                true,
            ),
            (Some(false), cfg(), false),
        ] {
            let o = Options {
                file_separators: flag,
                ..none()
            };
            assert_eq!(o.separators(&c), want, "{flag:?} {c:?}");
            assert_eq!(
                o.app(&[], &c, false).file_separators,
                want,
                "{flag:?} {c:?}"
            );
        }
    }

    /// --status-line and --no-status-line decide; without one the
    /// config does; without either the line is drawn.
    #[test]
    fn status_line() {
        for (flag, c, hide) in [
            (None, cfg(), false),
            (
                None,
                Config {
                    status_line: Some(false),
                    ..cfg()
                },
                true,
            ),
            (
                Some(true),
                Config {
                    status_line: Some(false),
                    ..cfg()
                },
                false,
            ),
            (Some(false), cfg(), true),
        ] {
            let o = Options {
                status_line: flag,
                ..none()
            };
            assert_eq!(o.app(&[], &c, false).hide_status, hide, "{flag:?} {c:?}");
        }
    }

    /// --view-style decides; without it the config does.
    #[test]
    fn app_view_style() {
        let plain = Config {
            view_style: Some(ViewStyle::Plain),
            ..cfg()
        };
        for (flag, c, want) in [
            (None, cfg(), ViewStyle::Color),
            (None, plain.clone(), ViewStyle::Plain),
            (Some(ViewStyle::Color), plain.clone(), ViewStyle::Color),
            (Some(ViewStyle::Plain), cfg(), ViewStyle::Plain),
        ] {
            let o = Options {
                view_style: flag,
                ..none()
            };
            assert_eq!(o.view(&c), want, "{flag:?} {c:?}");
            assert_eq!(o.app(&[], &c, false).view_style, want, "{flag:?} {c:?}");
        }
    }

    /// Each file is named as given; - and no files are stdin.
    #[test]
    fn app_names() {
        let names = |files: &[&str]| {
            let files: Vec<OsString> = files.iter().map(OsString::from).collect();
            none().app(&files, &cfg(), false).names
        };
        assert_eq!(names(&[]), [STDIN]);
        assert_eq!(names(&["a", "-", "b"]), ["a", STDIN, "b"]);
    }
}
