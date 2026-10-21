//! By hand: +N and +G are not flags any parser knows, and a setting
//! flag is a config line, so one parser reads the value.

use std::borrow::Cow;
use std::ffi::OsString;

use crate::app::StartScreen;
use crate::cli::{CmdFlag, Options, completion};
use crate::input::{config, setting};
use crate::sys::words;

/// Parses `$VEDI`, flags for every run, and then the command line,
/// which wins; a start position in `env` gives way to one in `args`.
/// Returns the options and the files, their bytes as given.
pub fn parse(args: &[OsString], env: &str) -> Result<(Options, Vec<OsString>), String> {
    let mut o = Options::default();
    words::split(env)
        .and_then(|w| o.parse(&w.into_iter().map(OsString::from).collect::<Vec<_>>(), true))
        .map_err(|e| format!("VEDI: {e}"))?;
    let from_env = (
        o.start_line.take(),
        std::mem::take(&mut o.follow),
        o.screen.take(),
    );
    let files = o.parse(args, false)?;
    if o.start_line.is_none() && !o.follow && o.screen.is_none() {
        (o.start_line, o.follow, o.screen) = from_env;
    }
    Ok((o, files))
}

/// An integer of at least `min`.
fn int(v: &str, min: i64) -> Option<usize> {
    v.parse::<i64>()
        .ok()
        .filter(|&n| n >= min)
        .and_then(|n| usize::try_from(n).ok())
}

/// The value of flag `name`: after its = in the arg, else the next arg.
fn value<'a>(
    name: &str,
    val: Option<&'a str>,
    args: &'a [OsString],
    i: &mut usize,
) -> Result<Cow<'a, str>, String> {
    match val {
        Some(v) => Ok(Cow::Borrowed(v)),
        None => {
            let v = args
                .get(*i)
                .ok_or_else(|| format!("{name} needs an argument"))?;
            *i += 1;
            Ok(v.to_string_lossy())
        }
    }
}

impl Options {
    /// Lays `args` over `self` and returns the files. `env` is for the
    /// words of `$VEDI`, which may only be flags, and not -h, -v or
    /// --completion.
    fn parse(&mut self, args: &[OsString], env: bool) -> Result<Vec<OsString>, String> {
        let mut files = Vec::new();
        let mut i = 0;
        while i < args.len() {
            // A flag is read as text; a file name keeps its bytes.
            let arg = &args[i];
            let lossy = arg.to_string_lossy();
            let a = &*lossy;
            i += 1;
            let (name, val) = match a.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (a, None),
            };
            if let Some((verb, fixed)) = setting::flag_verb(name) {
                let val = match fixed {
                    Some(_) if val.is_some() => return Err(format!("{name} takes no value")),
                    Some(v) => Cow::Borrowed(v),
                    None => value(name, val, args, &mut i)?,
                };
                self.setting(name, verb.name, &val)?;
                continue;
            }
            if let Some(set) = value_flag(name) {
                if env && name == "--completion" {
                    return Err(format!("{name:?} is not allowed"));
                }
                let val = value(name, val, args, &mut i)?;
                set(self, &val).ok_or_else(|| format!("bad {name} {val:?}"))?;
                continue;
            }
            match a {
                "-S" => self.setting(a, "wrap", "no")?,
                "-F" | "--quit-if-one-page" => self.quit_if_one_page = true,
                "--no-quit-if-one-page" => self.quit_if_one_page = false,
                "--no-clipboard-cmd" => self.clipboard_cmd = CmdFlag::Off,
                "--no-open-cmd" => self.open_cmd = CmdFlag::Off,
                "+G" => self.follow = true,
                _ if a.starts_with('+') => {
                    self.start_line =
                        Some(int(&a[1..], 1).ok_or_else(|| format!("bad line number {a:?}"))?);
                }
                "-h" | "--help" | "-v" | "--version" if env => {
                    return Err(format!("{a:?} is not allowed"));
                }
                "-h" | "--help" => self.help = true,
                "-v" | "--version" => self.version = true,
                _ if env && (a == "--" || a == "-" || !a.starts_with('-')) => {
                    return Err(format!("{a:?} is not a flag"));
                }
                "--" => {
                    files.extend_from_slice(&args[i..]);
                    break;
                }
                _ if a.starts_with('-') && a != "-" => {
                    return Err(format!("unknown flag {a:?}"));
                }
                _ => files.push(arg.clone()),
            }
        }
        if self.screen.is_some() && (self.start_line.is_some() || self.follow) {
            return Err("--scrolled-by and --cursor-* cannot be combined with +N or +G".into());
        }
        if self.start_line.is_some() && self.follow {
            return Err("+N and +G cannot be combined".into());
        }
        Ok(files)
    }

    /// Keeps the config line `verb val` for flag `flag`, checked now so
    /// a bad flag is reported before a bad file. The error names the
    /// flag in the verb's place and quotes the value, which a flag in
    /// $VEDI or an alias leaves out of sight.
    fn setting(&mut self, flag: &str, verb: &str, val: &str) -> Result<(), String> {
        let line = format!("{verb} {val}");
        config::parse_line(&line, &mut config::Config::default()).map_err(|e| {
            match e.strip_prefix(verb) {
                Some(rest) => format!("{flag}{rest}, not {val:?}"),
                None => e,
            }
        })?;
        self.lines.push(line);
        Ok(())
    }
}

type Set = fn(&mut Options, &str) -> Option<()>;

/// The setter for value flag `name`, as `--flag V` or `--flag=V`;
/// `None` for any other arg. A setter returns `None` for a bad value.
fn value_flag(name: &str) -> Option<Set> {
    fn screen(o: &mut Options) -> &mut StartScreen {
        o.screen.get_or_insert_with(StartScreen::default)
    }
    let set: Set = match name {
        "--scrolled-by" => |o, v| {
            screen(o).scrolled_by = int(v, 0)?;
            Some(())
        },
        "--cursor-row" => |o, v| {
            screen(o).cursor_row = int(v, 1)?;
            Some(())
        },
        "--cursor-col" => |o, v| {
            screen(o).cursor_col = int(v, 1)?;
            Some(())
        },
        "--clipboard-cmd" => |o, v| {
            o.clipboard_cmd = CmdFlag::Cmd(v.to_string());
            Some(())
        },
        "--open-cmd" => |o, v| {
            o.open_cmd = CmdFlag::Cmd(v.to_string());
            Some(())
        },
        "--config" => |o, v| {
            o.config = Some(v.into());
            Some(())
        },
        "--completion" => |o, v| {
            completion(v)?;
            o.completion = Some(v.to_string());
            Some(())
        },
        _ => return None,
    };
    Some(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    fn oss(s: &[&str]) -> Vec<OsString> {
        s.iter().map(OsString::from).collect()
    }

    fn cmd(s: &str) -> CmdFlag {
        CmdFlag::Cmd(s.to_string())
    }

    fn screen(o: &mut Options, row: usize, col: usize) {
        o.screen = Some(StartScreen {
            cursor_row: row,
            cursor_col: col,
            ..StartScreen::default()
        });
    }

    /// A row: its name, the args (or $VEDI and the args), the change
    /// from the default options, and the files.
    type Case<'a> = (&'a str, &'a [&'a str], fn(&mut Options), &'a [&'a str]);
    type EnvCase<'a> = (
        &'a str,
        &'a str,
        &'a [&'a str],
        fn(&mut Options),
        &'a [&'a str],
    );

    fn want(set: fn(&mut Options), files: &[&str]) -> Result<(Options, Vec<OsString>), String> {
        let mut o = Options::default();
        set(&mut o);
        Ok((o, oss(files)))
    }

    /// A file name that is not UTF-8 is kept byte for byte, before
    /// and after --.
    #[test]
    fn parse_keeps_file_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let name = OsString::from_vec(vec![b'f', 0xff, b'.', b't', b'x', b't']);
        let args = [
            OsString::from("-S"),
            name.clone(),
            "--".into(),
            name.clone(),
        ];
        let (o, files) = parse(&args, "").unwrap();
        assert_eq!(o.lines, strs(&["wrap no"]));
        assert_eq!(files, [name.clone(), name]);
    }

    #[test]
    fn parse_args() {
        let cases: &[Case] = &[
            ("none", &[], |_| {}, &[]),
            ("file", &["a.txt"], |_| {}, &["a.txt"]),
            (
                "nowrap short",
                &["-S", "a"],
                |o| o.lines = strs(&["wrap no"]),
                &["a"],
            ),
            (
                "nowrap long",
                &["--no-wrap"],
                |o| o.lines = strs(&["wrap no"]),
                &[],
            ),
            ("wrap", &["--wrap"], |o| o.lines = strs(&["wrap yes"]), &[]),
            (
                "last wrap wins",
                &["-S", "--wrap"],
                |o| o.lines = strs(&["wrap no", "wrap yes"]),
                &[],
            ),
            (
                "wrap style word",
                &["--wrap-style", "word"],
                |o| o.lines = strs(&["wrap_style word"]),
                &[],
            ),
            (
                "wrap style char",
                &["--wrap-style=char"],
                |o| o.lines = strs(&["wrap_style char"]),
                &[],
            ),
            (
                "last wrap style wins",
                &["--wrap-style", "word", "--wrap-style", "char"],
                |o| o.lines = strs(&["wrap_style word", "wrap_style char"]),
                &[],
            ),
            (
                "wrap style leaves wrap",
                &["-S", "--wrap-style", "word"],
                |o| o.lines = strs(&["wrap no", "wrap_style word"]),
                &[],
            ),
            (
                "view style plain",
                &["--view-style", "plain"],
                |o| o.lines = strs(&["view_style plain"]),
                &[],
            ),
            (
                "view style color",
                &["--view-style=color"],
                |o| o.lines = strs(&["view_style color"]),
                &[],
            ),
            (
                "view style raw",
                &["--view-style", "raw"],
                |o| o.lines = strs(&["view_style raw"]),
                &[],
            ),
            ("plus G", &["+G"], |o| o.follow = true, &[]),
            ("plus N", &["+12", "f"], |o| o.start_line = Some(12), &["f"]),
            (
                "clipboard cmd",
                &["--clipboard-cmd", "pbcopy"],
                |o| o.clipboard_cmd = cmd("pbcopy"),
                &[],
            ),
            (
                "clipboard cmd eq",
                &["--clipboard-cmd=wl-copy -n"],
                |o| o.clipboard_cmd = cmd("wl-copy -n"),
                &[],
            ),
            (
                "config",
                &["--config", "/tmp/k.conf"],
                |o| o.config = Some("/tmp/k.conf".into()),
                &[],
            ),
            (
                "config eq",
                &["--config=k.conf", "f"],
                |o| o.config = Some("k.conf".into()),
                &["f"],
            ),
            (
                "quit if one page short",
                &["-F"],
                |o| o.quit_if_one_page = true,
                &[],
            ),
            (
                "quit if one page long",
                &["--quit-if-one-page"],
                |o| o.quit_if_one_page = true,
                &[],
            ),
            (
                "no quit if one page",
                &["--no-quit-if-one-page"],
                |_| {},
                &[],
            ),
            (
                "no quit if one page wins",
                &["-F", "--no-quit-if-one-page"],
                |_| {},
                &[],
            ),
            (
                "quit if one page wins",
                &["--no-quit-if-one-page", "-F"],
                |o| o.quit_if_one_page = true,
                &[],
            ),
            ("help", &["-h"], |o| o.help = true, &[]),
            ("version short", &["-v"], |o| o.version = true, &[]),
            ("version long", &["--version"], |o| o.version = true, &[]),
            ("dash dash", &["--", "-S"], |_| {}, &["-S"]),
            ("stdin dash", &["-"], |_| {}, &["-"]),
            (
                "scrolled by eq",
                &["--scrolled-by=0"],
                |o| screen(o, 0, 0),
                &[],
            ),
            (
                "cursor",
                &["--cursor-row", "20", "--cursor-col", "5"],
                |o| screen(o, 20, 5),
                &[],
            ),
            (
                "flag after file",
                &["a", "-S"],
                |o| o.lines = strs(&["wrap no"]),
                &["a"],
            ),
            (
                "clipboard cmd empty eq",
                &["--clipboard-cmd="],
                |o| o.clipboard_cmd = cmd(""),
                &[],
            ),
            (
                "no clipboard cmd",
                &["--no-clipboard-cmd"],
                |o| o.clipboard_cmd = CmdFlag::Off,
                &[],
            ),
            (
                "no clipboard cmd wins",
                &["--clipboard-cmd", "pbcopy", "--no-clipboard-cmd"],
                |o| o.clipboard_cmd = CmdFlag::Off,
                &[],
            ),
            (
                "clipboard cmd wins",
                &["--no-clipboard-cmd", "--clipboard-cmd=pbcopy"],
                |o| o.clipboard_cmd = cmd("pbcopy"),
                &[],
            ),
            (
                "auto reload",
                &["--auto-reload"],
                |o| o.lines = strs(&["auto_reload yes"]),
                &[],
            ),
            (
                "no auto reload",
                &["--no-auto-reload", "f"],
                |o| o.lines = strs(&["auto_reload no"]),
                &["f"],
            ),
            (
                "last auto reload wins",
                &["--no-auto-reload", "--auto-reload"],
                |o| o.lines = strs(&["auto_reload no", "auto_reload yes"]),
                &[],
            ),
            (
                "tab width",
                &["--tab-width", "4", "f"],
                |o| o.lines = strs(&["tab_width 4"]),
                &["f"],
            ),
            (
                "tab width eq",
                &["--tab-width=2"],
                |o| o.lines = strs(&["tab_width 2"]),
                &[],
            ),
            (
                "rows below",
                &["-F", "--one-page-rows-below", "2"],
                |o| (o.quit_if_one_page, o.lines) = (true, strs(&["one_page_rows_below 2"])),
                &[],
            ),
            (
                "rows below eq",
                &["--one-page-rows-below=3"],
                |o| o.lines = strs(&["one_page_rows_below 3"]),
                &[],
            ),
            (
                "open cmd",
                &["--open-cmd", "bat --color=always %s", "f"],
                |o| o.open_cmd = cmd("bat --color=always %s"),
                &["f"],
            ),
            (
                "open cmd eq",
                &["--open-cmd=cat %s"],
                |o| o.open_cmd = cmd("cat %s"),
                &[],
            ),
            (
                "no open cmd",
                &["--no-open-cmd", "f"],
                |o| o.open_cmd = CmdFlag::Off,
                &["f"],
            ),
            (
                "no open cmd wins",
                &["--open-cmd=bat %s", "--no-open-cmd"],
                |o| o.open_cmd = CmdFlag::Off,
                &[],
            ),
            (
                "completion",
                &["--completion", "bash"],
                |o| o.completion = Some("bash".into()),
                &[],
            ),
            (
                "completion eq",
                &["--completion=fish"],
                |o| o.completion = Some("fish".into()),
                &[],
            ),
            (
                "edge markers",
                &["--edge-markers"],
                |o| o.lines = strs(&["edge_markers yes"]),
                &[],
            ),
            (
                "no edge markers",
                &["--no-edge-markers"],
                |o| o.lines = strs(&["edge_markers no"]),
                &[],
            ),
            (
                "file separators",
                &["--file-separators"],
                |o| o.lines = strs(&["file_separators yes"]),
                &[],
            ),
            (
                "no file separators",
                &["--no-file-separators"],
                |o| o.lines = strs(&["file_separators no"]),
                &[],
            ),
            (
                "status line",
                &["--status-line"],
                |o| o.lines = strs(&["status_line yes"]),
                &[],
            ),
            (
                "no status line",
                &["--no-status-line"],
                |o| o.lines = strs(&["status_line no"]),
                &[],
            ),
        ];
        for &(name, args, set, files) in cases {
            assert_eq!(parse(&oss(args), ""), want(set, files), "{name}");
        }
    }

    #[test]
    fn parse_args_rejects() {
        let cases: &[(&str, &[&str])] = &[
            ("old nowrap spelling", &["--nowrap"]),
            ("missing wrap style", &["--wrap-style"]),
            ("bad wrap style", &["--wrap-style", "yes"]),
            ("missing view style", &["--view-style"]),
            ("bad view style", &["--view-style", "yes"]),
            ("missing config", &["--config"]),
            ("bad line", &["+0"]),
            ("bad line text", &["+abc"]),
            ("missing cmd", &["--clipboard-cmd"]),
            ("unknown flag", &["--bogus"]),
            ("bad scrolled by", &["--scrolled-by", "-1"]),
            ("bad cursor row", &["--cursor-row", "0"]),
            (
                "screen conflicts with plus N",
                &["+5", "--scrolled-by", "0"],
            ),
            ("screen conflicts with plus G", &["--cursor-row", "1", "+G"]),
            ("plus G conflicts with plus N", &["+G", "+5"]),
            ("missing rows below", &["--one-page-rows-below"]),
            ("bad rows below", &["--one-page-rows-below", "0"]),
            ("missing tab width", &["--tab-width"]),
            ("bad tab width", &["--tab-width", "0"]),
            ("bad tab width text", &["--tab-width", "four"]),
            ("tab width past a row", &["--tab-width", "65536"]),
            ("missing open cmd", &["--open-cmd"]),
            ("bad completion", &["--completion", "tcsh"]),
            ("missing completion", &["--completion"]),
            ("wrap with value", &["--wrap=yes"]),
            ("no before a value flag", &["--no-tab-width"]),
            ("config spelling", &["--wrap_style", "word"]),
            ("empty tab width", &["--tab-width="]),
            ("tab width with a space", &["--tab-width", "4 5"]),
            ("bad view style eq", &["--view-style=bold"]),
            ("bad rows below eq", &["--one-page-rows-below=0"]),
        ];
        for &(name, args) in cases {
            let got = parse(&oss(args), "");
            assert!(got.is_err(), "{name}: {got:?}, want an error");
        }
    }

    /// A setting flag's error is the config line's, naming the flag
    /// where the line names the verb.
    #[test]
    fn flag_errors_name_the_flag() {
        let cases: &[(&str, &[&str], &str)] = &[
            (
                "bad tab width",
                &["--tab-width", "0"],
                "--tab-width takes a number from 1 to 65535, not \"0\"",
            ),
            (
                "empty tab width",
                &["--tab-width="],
                "--tab-width takes a number from 1 to 65535, not \"\"",
            ),
            (
                "bad wrap style",
                &["--wrap-style", "yes"],
                "--wrap-style takes char or word, not \"yes\"",
            ),
            (
                "bad view style",
                &["--view-style=bold"],
                "--view-style takes color, plain or raw, not \"bold\"",
            ),
            (
                "bad rows below",
                &["--one-page-rows-below", "0"],
                "--one-page-rows-below takes a number from 1 to 65535, not \"0\"",
            ),
            (
                "value with a space",
                &["--tab-width", "4 5"],
                "--tab-width takes a number from 1 to 65535, not \"4 5\"",
            ),
            ("wrap with value", &["--wrap=yes"], "--wrap takes no value"),
            (
                "missing value",
                &["--wrap-style"],
                "--wrap-style needs an argument",
            ),
        ];
        for &(name, args, want) in cases {
            assert_eq!(parse(&oss(args), ""), Err(want.to_string()), "{name}");
        }
    }

    /// VEDI holds flags, split as a shell would; the command line is
    /// parsed after it, so it wins.
    #[test]
    fn parse_env() {
        let cases: &[EnvCase] = &[
            (
                "flags",
                "-F -S",
                &["f"],
                |o| (o.quit_if_one_page, o.lines) = (true, strs(&["wrap no"])),
                &["f"],
            ),
            ("blank", " \t ", &[], |_| {}, &[]),
            (
                "arg wins",
                "-S --tab-width 4",
                &["--wrap", "--tab-width=2"],
                |o| o.lines = strs(&["wrap no", "tab_width 4", "wrap yes", "tab_width 2"]),
                &[],
            ),
            (
                "no quit if one page beats env",
                "-F",
                &["--no-quit-if-one-page", "f"],
                |_| {},
                &["f"],
            ),
            (
                "no open cmd beats env",
                "--open-cmd 'bat %s'",
                &["--no-open-cmd"],
                |o| o.open_cmd = CmdFlag::Off,
                &[],
            ),
            (
                "no clipboard cmd beats env",
                "--clipboard-cmd pbcopy",
                &["--no-clipboard-cmd"],
                |o| o.clipboard_cmd = CmdFlag::Off,
                &[],
            ),
            (
                "single quotes",
                "--clipboard-cmd 'xclip -selection clipboard'",
                &[],
                |o| o.clipboard_cmd = cmd("xclip -selection clipboard"),
                &[],
            ),
            (
                "double quotes",
                r#"--clipboard-cmd "sh -c 'cat >f'""#,
                &[],
                |o| o.clipboard_cmd = cmd("sh -c 'cat >f'"),
                &[],
            ),
            (
                "quotes join",
                r#"--clipboard-cmd="wl-copy -n""#,
                &[],
                |o| o.clipboard_cmd = cmd("wl-copy -n"),
                &[],
            ),
            (
                "backslash",
                r"--config my\ vedi.conf",
                &[],
                |o| o.config = Some("my vedi.conf".into()),
                &[],
            ),
            (
                "backslash in double quotes",
                r#"--clipboard-cmd "a \"b\" \\ \c""#,
                &[],
                |o| o.clipboard_cmd = cmd(r#"a "b" \ \c"#),
                &[],
            ),
            (
                "backslash in single quotes",
                r"--clipboard-cmd 'a\b'",
                &[],
                |o| o.clipboard_cmd = cmd(r"a\b"),
                &[],
            ),
            (
                "empty quotes",
                "--clipboard-cmd ''",
                &["f"],
                |o| o.clipboard_cmd = cmd(""),
                &["f"],
            ),
            (
                "open cmd",
                "-F --open-cmd 'bat --color=always --paging=never %s'",
                &[],
                |o| {
                    (o.quit_if_one_page, o.open_cmd) =
                        (true, cmd("bat --color=always --paging=never %s"))
                },
                &[],
            ),
            ("plus G", "+G", &[], |o| o.follow = true, &[]),
            (
                "plus N beats plus G",
                "+G",
                &["+5"],
                |o| o.start_line = Some(5),
                &[],
            ),
            (
                "plus G beats plus N",
                "+5",
                &["+G"],
                |o| o.follow = true,
                &[],
            ),
            (
                "screen beats plus G",
                "+G",
                &["--cursor-row", "2"],
                |o| screen(o, 2, 0),
                &[],
            ),
            (
                "plus N beats screen",
                "--scrolled-by 1",
                &["+3"],
                |o| o.start_line = Some(3),
                &[],
            ),
            (
                "screen replaces screen",
                "--scrolled-by 1",
                &["--cursor-row", "2"],
                |o| screen(o, 2, 0),
                &[],
            ),
        ];
        for &(name, env, args, set, files) in cases {
            assert_eq!(parse(&oss(args), env), want(set, files), "{name}");
        }
    }

    /// VEDI takes flags only, and its errors name it.
    #[test]
    fn parse_env_rejects() {
        for (name, env, want) in [
            ("file", "-S a.txt", r#"VEDI: "a.txt" is not a flag"#),
            ("stdin dash", "-", r#"VEDI: "-" is not a flag"#),
            ("dash dash", "-- -S", r#"VEDI: "--" is not a flag"#),
            ("help", "-h", r#"VEDI: "-h" is not allowed"#),
            (
                "version",
                "--version",
                r#"VEDI: "--version" is not allowed"#,
            ),
            (
                "completion",
                "--completion bash",
                r#"VEDI: "--completion" is not allowed"#,
            ),
            ("unknown flag", "--bogus", r#"VEDI: unknown flag "--bogus""#),
            (
                "bad value",
                "--tab-width 0",
                "VEDI: --tab-width takes a number from 1 to 65535, not \"0\"",
            ),
            (
                "yes no with value",
                "--wrap=yes",
                "VEDI: --wrap takes no value",
            ),
            (
                "missing value",
                "--tab-width",
                "VEDI: --tab-width needs an argument",
            ),
            ("conflict", "+G +5", "VEDI: +N and +G cannot be combined"),
            (
                "open single quote",
                "--clipboard-cmd 'pbcopy",
                "VEDI: unclosed '",
            ),
            (
                "open double quote",
                r#"--clipboard-cmd "pbcopy"#,
                r#"VEDI: unclosed ""#,
            ),
            ("trailing backslash", r"-S \", r"VEDI: trailing \"),
        ] {
            assert_eq!(parse(&oss(&["f"]), env), Err(want.to_string()), "{name}");
        }
    }
}
