//! vedi.conf: one directive per line, kitty style.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use glob::Pattern;

use crate::input::action::Command;
use crate::input::key::{Key, Mods};
use crate::input::keymap::{Binding, DEFAULTS, Keymap};
use crate::input::setting::{self, Settings};
use crate::sys::words;
use crate::view::layout::Mode;

/// What a file sets.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Config {
    /// map lines, in order.
    pub keys: Vec<Binding>,
    /// clear_all_shortcuts: the map starts empty.
    pub clear: bool,
    /// Copy pipes to this, not OSC 52.
    pub clipboard_cmd: Option<String>,
    /// open_cmd lines, in order.
    pub open_cmds: Vec<OpenCmd>,
    /// The setting lines laid over the defaults, later lines winning.
    pub settings: Settings,
}

/// An open_cmd line: files whose base name matches `pattern`, both
/// lowercased, are read by running `argv`, %s the name; `None` is for
/// the files no pattern matches.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OpenCmd {
    pub pattern: Option<Pattern>,
    pub argv: Vec<String>,
}

/// A bad line of a file named `name`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConfigError {
    pub name: String,
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}: {}", self.name, self.line, self.msg)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug)]
pub enum LoadError {
    NotFound(io::Error),
    Io(io::Error),
    Parse(ConfigError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LoadError::NotFound(e) | LoadError::Io(e) => write!(f, "{e}"),
            LoadError::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl Config {
    /// The command `name` is read through: the last pattern line
    /// matching its base name, ignoring case, else the last plain line,
    /// else `None` to read it as it is.
    pub fn open_cmd_for(&self, name: &str) -> Option<&[String]> {
        // Both sides lowercased: the glob crate folds ASCII only.
        let base = Path::new(name)
            .file_name()
            .map_or(name.into(), |b| b.to_string_lossy())
            .to_lowercase();
        let mut plain = None;
        for o in self.open_cmds.iter().rev() {
            match &o.pattern {
                None => {
                    plain.get_or_insert(&o.argv[..]);
                }
                Some(p) if p.matches(&base) => return Some(&o.argv),
                Some(_) => {}
            }
        }
        plain
    }

    /// The bindings in effect: the defaults, or none after
    /// clear_all_shortcuts, with the map lines laid over.
    pub fn keymap(&self, macos: bool) -> Keymap {
        if self.clear {
            return Keymap::new(self.keys.clone());
        }
        default_keymap(macos).apply(&self.keys)
    }
}

/// The built-in key map; off macOS the Cmd lines are left out.
pub fn default_keymap(macos: bool) -> Keymap {
    let c = parse("defaults.conf", DEFAULTS).expect("defaults.conf parses");
    Keymap::new(
        c.keys
            .into_iter()
            .filter(|b| macos || !b.key.mods.contains(Mods::CMD))
            .collect(),
    )
}

/// Applies `src` line by line.
pub fn parse(name: &str, src: &str) -> Result<Config, ConfigError> {
    let mut c = Config::default();
    for (i, line) in src.split('\n').enumerate() {
        parse_line(line, &mut c).map_err(|msg| ConfigError {
            name: name.to_string(),
            line: i + 1,
            msg,
        })?;
    }
    Ok(c)
}

/// Applies one line to `c` and names its verb: `None` for a blank line
/// or a comment. The error has no file position.
pub fn parse_line<'a>(line: &'a str, c: &mut Config) -> Result<Option<&'a str>, String> {
    let f: Vec<&str> = line.split_whitespace().collect();
    let Some(&verb) = f.first() else {
        return Ok(None);
    };
    if verb.starts_with('#') {
        return Ok(None);
    }
    let rest = line.trim()[verb.len()..].trim();
    // The one word after the verb; "" for none or several.
    let arg = match f[..] {
        [_, arg] => arg,
        _ => "",
    };
    // A verb with values is checked here, so its arm need not.
    if let Some(v) = setting::lookup(verb)
        && !v.values().contains(&arg)
    {
        return Err(format!("{verb} takes {}", setting::or_list(v.values())));
    }
    let positive = || match arg.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("{verb} takes a positive number")),
    };
    let s = &mut c.settings;
    match verb {
        "map" => {
            if f.len() != 3 && !(f.len() == 4 && f[2] == "cycle") {
                return Err("map takes a key and an action".to_string());
            }
            let key: Key = f[1].parse().map_err(|e| format!("{e}"))?;
            let cmd = Command::parse_binding(&f[2..].join(" ")).map_err(|e| format!("{e}"))?;
            c.keys.push(Binding { key, cmd });
        }
        "clear_all_shortcuts" => {
            if f.len() != 1 {
                return Err("clear_all_shortcuts takes nothing".to_string());
            }
            c.keys.clear();
            c.clear = true;
        }
        "auto_reload" => s.auto_reload = arg == "yes",
        "wrap" => {
            s.mode = if arg == "yes" {
                Mode::Wrap
            } else {
                Mode::NoWrap
            }
        }
        "edge_markers" => s.edge_markers = arg == "yes",
        "file_separators" => s.file_separators = arg == "yes",
        "status_line" => s.status_line = arg == "yes",
        "wrap_style" => s.wrap_style = arg.parse().expect("checked"),
        "view_style" => s.view_style = arg.parse().expect("checked"),
        "clipboard_cmd" => {
            if rest.is_empty() {
                return Err("clipboard_cmd takes a command".to_string());
            }
            c.clipboard_cmd = Some(rest.to_string());
        }
        "open_cmd" => c.open_cmds.push(parse_open_cmd(rest)?),
        "tab_width" => s.tab_width = positive()?,
        "one_page_rows_below" => s.one_page_rows_below = positive()?,
        _ => return Err(format!("unknown verb {verb:?}")),
    }
    Ok(Some(verb))
}

/// What follows open_cmd: a pattern, when the first word has a glob
/// character, then the command.
fn parse_open_cmd(s: &str) -> Result<OpenCmd, String> {
    let mut argv = words::split(s).map_err(|e| format!("open_cmd: {e}"))?;
    let mut pattern = None;
    if argv.first().is_some_and(|w| w.contains(['*', '?', '['])) {
        let first = argv.remove(0);
        let p = Pattern::new(&first.to_lowercase())
            .map_err(|_| format!("open_cmd: bad pattern {first}"))?;
        pattern = Some(p);
    }
    if argv.is_empty() {
        return Err("open_cmd takes a command".to_string());
    }
    Ok(OpenCmd { pattern, argv })
}

/// Where the file is looked for: $XDG_CONFIG_HOME/vedi/vedi.conf, else
/// ~/.config/vedi/vedi.conf. Empty when there is no home directory.
pub fn path() -> PathBuf {
    path_in(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn path_in(xdg: Option<OsString>, home: Option<OsString>) -> PathBuf {
    let dir = match (xdg, home) {
        (Some(xdg), _) if !xdg.is_empty() => PathBuf::from(xdg),
        (_, Some(home)) if !home.is_empty() => Path::new(&home).join(".config"),
        _ => return PathBuf::new(),
    };
    dir.join("vedi").join("vedi.conf")
}

/// Reads and parses the file at `path`; an empty path is an empty
/// config.
pub fn load(path: &Path) -> Result<Config, LoadError> {
    if path.as_os_str().is_empty() {
        return Ok(Config::default());
    }
    let src = fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => LoadError::NotFound(e),
        _ => LoadError::Io(e),
    })?;
    parse(&path.display().to_string(), &String::from_utf8_lossy(&src)).map_err(LoadError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::help::{Row, Section};
    use crate::input::key::KeyCode;
    use crate::text::ansi::ViewStyle;
    use crate::view::layout::WrapStyle;

    fn key(name: &str) -> Key {
        name.parse().unwrap()
    }

    fn cmd(name: &str) -> Option<Command> {
        Command::parse_binding(name).unwrap()
    }

    fn bind(k: &str, c: &str) -> Binding {
        Binding {
            key: key(k),
            cmd: cmd(c),
        }
    }

    fn parse_err(src: &str) -> String {
        parse("vedi.conf", src).unwrap_err().to_string()
    }

    fn strings(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_keys() {
        let src = "# vim\n\nmap h left\n\tmap  L\tselect_right \nmap ? none\r\nmap Alt+w cycle wrap_style\n";
        let c = parse("vedi.conf", src).unwrap();
        assert_eq!(
            c.keys,
            [
                Binding {
                    key: Key::new(KeyCode::Char('h'), Mods::NONE),
                    cmd: cmd("left")
                },
                Binding {
                    key: Key::new(KeyCode::Char('L'), Mods::NONE),
                    cmd: cmd("select_right")
                },
                Binding {
                    key: Key::new(KeyCode::Char('?'), Mods::NONE),
                    cmd: None
                },
                Binding {
                    key: Key::new(KeyCode::Char('w'), Mods::ALT),
                    cmd: cmd("cycle wrap_style")
                },
            ]
        );
        assert_eq!(parse("empty", ""), Ok(Config::default()));
        let cycle = "vedi.conf:1: cycle takes wrap, wrap_style, edge_markers, file_separators, auto_reload, status_line, view_style or open_cmd";
        let bad = [
            (
                "map h left\nbind j down\n",
                r#"vedi.conf:2: unknown verb "bind""#,
            ),
            (
                "clear_all_shortcuts yes\n",
                "vedi.conf:1: clear_all_shortcuts takes nothing",
            ),
            ("map h\n", "vedi.conf:1: map takes a key and an action"),
            (
                "map h left now\n",
                "vedi.conf:1: map takes a key and an action",
            ),
            ("\n\nmap F1 help\n", r#"vedi.conf:3: unknown key "F1""#),
            ("map h foo\n", r#"vedi.conf:1: unknown action "foo""#),
            (
                "map Shift+h left\n",
                r#"vedi.conf:1: "Shift+h": modifiers on a character key"#,
            ),
            (
                "map h select_copy\n",
                r#"vedi.conf:1: unknown action "select_copy""#,
            ),
            ("map h cycle\n", cycle),
            ("map h cycle tab_width\n", cycle),
            (
                "map h cycle wrap yes\n",
                "vedi.conf:1: map takes a key and an action",
            ),
            (
                "map h toggle_wrap\n",
                r#"vedi.conf:1: unknown action "toggle_wrap""#,
            ),
        ];
        for (src, want) in bad {
            assert_eq!(parse_err(src), want, "parse({src:?})");
        }
    }

    /// One line applies on its own, naming its verb, with the error bare
    /// of the file position that `parse` adds.
    #[test]
    fn parse_one_line() {
        let mut c = Config::default();
        assert_eq!(
            parse_line("  wrap_style  word ", &mut c),
            Ok(Some("wrap_style"))
        );
        assert_eq!(c.settings.wrap_style, WrapStyle::Word);
        assert_eq!(
            parse_line("clipboard_cmd sh -c false", &mut c),
            Ok(Some("clipboard_cmd"))
        );
        assert_eq!(c.clipboard_cmd.as_deref(), Some("sh -c false"));
        for blank in ["", "   ", "# a comment"] {
            let mut c = Config::default();
            assert_eq!(parse_line(blank, &mut c), Ok(None), "{blank:?}");
            assert_eq!(c, Config::default(), "{blank:?}");
        }
        for (src, want) in [
            ("wrap_style wide", "wrap_style takes char or word"),
            ("bind j down", r#"unknown verb "bind""#),
        ] {
            let mut c = Config::default();
            assert_eq!(parse_line(src, &mut c), Err(want.to_string()), "{src:?}");
        }
    }

    /// clear_all_shortcuts drops what was bound before it, the defaults
    /// included; map lines after it build the map from nothing.
    #[test]
    fn clear_all_shortcuts() {
        let c = parse("vedi.conf", "map j quit\nclear_all_shortcuts\nmap x quit\n").unwrap();
        let want = vec![bind("x", "quit")];
        assert!(c.clear);
        assert_eq!(c.keys, want);
        assert_eq!(c.keymap(true), Keymap::new(want));
        let c = parse("vedi.conf", "clear_all_shortcuts\n").unwrap();
        assert_eq!(c.keymap(true).bindings(), []);
        let c = parse("vedi.conf", "map x quit\n").unwrap();
        assert_eq!(c.keymap(false), default_keymap(false).apply(&c.keys));
    }

    #[test]
    fn default_os() {
        let mac = default_keymap(true);
        let other = default_keymap(false);
        let stripped: Vec<&Binding> = mac
            .bindings()
            .iter()
            .filter(|b| !b.key.mods.contains(Mods::CMD))
            .collect();
        assert_ne!(
            stripped.len(),
            mac.bindings().len(),
            "macOS map has no Cmd keys"
        );
        assert_eq!(stripped, other.bindings().iter().collect::<Vec<_>>());
        // The map keeps one binding per key; the file must too.
        let keys = parse("defaults.conf", DEFAULTS).unwrap().keys;
        for (i, b) in keys.iter().enumerate() {
            assert!(
                keys[i + 1..].iter().all(|o| o.key != b.key),
                "{} bound twice",
                b.key
            );
        }
    }

    /// What lookup adds to parsing: the event side of normalizing.
    #[test]
    fn lookup() {
        use KeyCode::*;
        let ch = KeyCode::Char;
        let tests = [
            ("up", false, Key::new(Up, Mods::NONE), "up"),
            (
                "shift up extends",
                false,
                Key::new(Up, Mods::SHIFT),
                "select_up",
            ),
            (
                "ctrl on a named key",
                false,
                Key::new(Left, Mods::CTRL),
                "word_left",
            ),
            (
                "alt on a rune",
                false,
                Key::new(ch('b'), Mods::ALT),
                "word_left",
            ),
            ("ctrl f", false, Key::new(ch('f'), Mods::CTRL), "page_down"),
            (
                "ctrl f as a control character",
                false,
                Key::new(ch('\x06'), Mods::NONE),
                "page_down",
            ),
            (
                "shift on a character",
                false,
                Key::new(ch('<'), Mods::SHIFT),
                "first",
            ),
            (
                "G never extends",
                false,
                Key::new(ch('G'), Mods::SHIFT),
                "last",
            ),
            ("enter", false, Key::new(Enter, Mods::NONE), "copy_and_quit"),
            ("unbound rune", false, Key::new(ch('z'), Mods::NONE), "none"),
            (
                "unlisted modifier is unbound",
                false,
                Key::new(Home, Mods::CTRL),
                "none",
            ),
            ("cmd c", true, Key::new(ch('c'), Mods::CMD), "copy"),
            (
                "cmd shift G as a capital",
                true,
                Key::new(ch('G'), Mods::CMD | Mods::SHIFT),
                "search_prev",
            ),
            (
                "cmd shift up extends",
                true,
                Key::new(Up, Mods::CMD | Mods::SHIFT),
                "select_first",
            ),
            (
                "cmd q is unbound",
                true,
                Key::new(ch('q'), Mods::CMD),
                "none",
            ),
            (
                "cmd c elsewhere",
                false,
                Key::new(ch('c'), Mods::CMD),
                "none",
            ),
        ];
        for (name, macos, k, want) in tests {
            assert_eq!(default_keymap(macos).lookup(k), cmd(want), "{name}");
        }
    }

    #[test]
    fn lookup_applied() {
        let m = default_keymap(false).apply(&[
            bind("h", "left"),
            bind("L", "select_right"),
            bind("Alt+<", "first"),
        ]);
        // The kitty protocol reports Alt+< as , with Shift and Alt.
        let kitty = Key::new(KeyCode::Char(','), Mods::SHIFT | Mods::ALT);
        assert_eq!(m.lookup(kitty), cmd("first"));
        assert_eq!(m.lookup(key("h")), cmd("left"));
        let shift_l = Key::new(KeyCode::Char('L'), Mods::SHIFT);
        assert_eq!(m.lookup(shift_l), cmd("select_right"));
    }

    #[test]
    fn config_path() {
        assert_eq!(
            path_in(Some("/xdg".into()), Some("/home/u".into())),
            PathBuf::from("/xdg/vedi/vedi.conf")
        );
        assert_eq!(
            path_in(Some("".into()), Some("/home/u".into())),
            PathBuf::from("/home/u/.config/vedi/vedi.conf")
        );
        assert_eq!(path_in(None, None), PathBuf::new());
    }

    /// A directory removed on drop.
    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn load_file() {
        let dir = TempDir(std::env::temp_dir().join(format!("vedi-config-{}", std::process::id())));
        fs::create_dir_all(&dir.0).unwrap();
        let path = dir.0.join("vedi.conf");
        assert!(matches!(load(&path), Err(LoadError::NotFound(_))));
        assert_eq!(load(Path::new("")).unwrap(), Config::default());
        fs::write(&path, "map h left\n").unwrap();
        let c = load(&path).unwrap();
        assert_eq!(
            default_keymap(false).apply(&c.keys).lookup(key("h")),
            cmd("left")
        );
        fs::write(&path, "map h left\n\nmap x foo\n").unwrap();
        let err = load(&path).unwrap_err();
        assert!(matches!(err, LoadError::Parse(_)));
        assert_eq!(
            err.to_string(),
            format!(r#"{}:3: unknown action "foo""#, path.display())
        );
        assert!(
            matches!(load(&dir.0), Err(LoadError::Io(_))),
            "a directory reads"
        );
    }

    /// Each yes-or-no setting: its default without a line, the last
    /// line wins, and anything but yes or no is an error.
    fn check_yes_no(name: &str, get: fn(&Settings) -> bool) {
        let tests = [
            (String::new(), get(&Settings::default())),
            (format!("{name} no\n"), false),
            (format!("{name} yes\n"), true),
            (format!("{name} no\n{name} yes\n"), true),
            (format!("{name} yes\nmap x quit\n{name} no\n"), false),
        ];
        for (src, want) in tests {
            assert_eq!(
                parse("vedi.conf", &src).map(|c| get(&c.settings)),
                Ok(want),
                "{src:?}"
            );
        }
        let want = format!("vedi.conf:1: {name} takes yes or no");
        for arg in ["", " maybe", " no yes", " word"] {
            assert_eq!(parse_err(&format!("{name}{arg}\n")), want, "{name}{arg}");
        }
    }

    #[test]
    fn auto_reload() {
        check_yes_no("auto_reload", |s| s.auto_reload);
    }

    #[test]
    fn wrap() {
        check_yes_no("wrap", |s| s.mode == Mode::Wrap);
    }

    #[test]
    fn edge_markers() {
        check_yes_no("edge_markers", |s| s.edge_markers);
    }

    #[test]
    fn file_separators() {
        check_yes_no("file_separators", |s| s.file_separators);
    }

    #[test]
    fn status_line() {
        check_yes_no("status_line", |s| s.status_line);
    }

    #[test]
    fn wrap_style() {
        let tests = [
            ("", WrapStyle::Char),
            ("wrap_style word\n", WrapStyle::Word),
            ("wrap_style char\n", WrapStyle::Char),
            ("wrap_style word\nwrap_style char\n", WrapStyle::Char),
            ("wrap_style word\nwrap no\n", WrapStyle::Word),
        ];
        for (src, want) in tests {
            assert_eq!(
                parse("vedi.conf", src).map(|c| c.settings.wrap_style),
                Ok(want),
                "{src:?}"
            );
        }
        for src in ["wrap_style\n", "wrap_style yes\n", "wrap_style word char\n"] {
            assert_eq!(
                parse_err(src),
                "vedi.conf:1: wrap_style takes char or word",
                "{src:?}"
            );
        }
    }

    #[test]
    fn view_style() {
        let tests = [
            ("", ViewStyle::Color),
            ("view_style plain\n", ViewStyle::Plain),
            ("view_style color\n", ViewStyle::Color),
            ("view_style raw\n", ViewStyle::Raw),
            ("view_style plain\nview_style color\n", ViewStyle::Color),
        ];
        for (src, want) in tests {
            assert_eq!(
                parse("vedi.conf", src).map(|c| c.settings.view_style),
                Ok(want),
                "{src:?}"
            );
        }
        for src in [
            "view_style\n",
            "view_style yes\n",
            "view_style plain color\n",
        ] {
            assert_eq!(
                parse_err(src),
                "vedi.conf:1: view_style takes color, plain or raw",
                "{src:?}"
            );
        }
    }

    /// clipboard_cmd takes the rest of the line, spaces and all; the
    /// last line wins; a bare verb is an error.
    #[test]
    fn clipboard_cmd() {
        let tests = [
            ("", None),
            ("clipboard_cmd pbcopy\n", Some("pbcopy")),
            (
                "clipboard_cmd  xclip -selection clipboard  \n",
                Some("xclip -selection clipboard"),
            ),
            ("\tclipboard_cmd\twl-copy\n", Some("wl-copy")),
            (
                "clipboard_cmd pbcopy\nclipboard_cmd wl-copy\n",
                Some("wl-copy"),
            ),
        ];
        for (src, want) in tests {
            let c = parse("vedi.conf", src).unwrap();
            assert_eq!(c.clipboard_cmd.as_deref(), want, "{src:?}");
        }
        assert_eq!(
            parse_err("clipboard_cmd\n"),
            "vedi.conf:1: clipboard_cmd takes a command"
        );
    }

    /// open_cmd takes a command, split as a shell would, or a glob on
    /// the file's base name and then the command; the pattern may be
    /// quoted. A bare verb, a pattern alone and a bad pattern are errors.
    #[test]
    fn open_cmd() {
        let o = |pattern: Option<&str>, argv: &[&str]| OpenCmd {
            pattern: pattern.map(|p| Pattern::new(p).unwrap()),
            argv: strings(argv),
        };
        let tests = [
            ("", vec![]),
            (
                "open_cmd bat --color=always --paging=never %s\n",
                vec![o(None, &["bat", "--color=always", "--paging=never", "%s"])],
            ),
            ("\topen_cmd\t cat  '%s' \n", vec![o(None, &["cat", "%s"])]),
            (
                "open_cmd *.md mdcat %s\nopen_cmd cat %s\n",
                vec![o(Some("*.md"), &["mdcat", "%s"]), o(None, &["cat", "%s"])],
            ),
            // Kept lowercased, to match a lowercased name.
            (
                "open_cmd [Mm]akefile cat %s\n",
                vec![o(Some("[mm]akefile"), &["cat", "%s"])],
            ),
            (
                "open_cmd '*.md' cat %s\n",
                vec![o(Some("*.md"), &["cat", "%s"])],
            ),
            (
                "open_cmd \"my *.md\" cat %s\n",
                vec![o(Some("my *.md"), &["cat", "%s"])],
            ),
        ];
        for (src, want) in tests {
            assert_eq!(
                parse("vedi.conf", src).map(|c| c.open_cmds),
                Ok(want),
                "{src:?}"
            );
        }
        for (src, want) in [
            ("open_cmd\n", "vedi.conf:1: open_cmd takes a command"),
            ("open_cmd *.md\n", "vedi.conf:1: open_cmd takes a command"),
            (
                "open_cmd [md cat %s\n",
                "vedi.conf:1: open_cmd: bad pattern [md",
            ),
            ("open_cmd cat '%s\n", "vedi.conf:1: open_cmd: unclosed '"),
        ] {
            assert_eq!(parse_err(src), want, "{src:?}");
        }
    }

    /// A file is read through the last pattern line matching its base
    /// name, ignoring case, else the last plain line, else as it is.
    #[test]
    fn open_cmd_for() {
        let c = parse(
            "vedi.conf",
            "
open_cmd *.md mdcat %s
open_cmd cat %s
open_cmd *.py bat %s
open_cmd *.md glow %s
open_cmd less %s
",
        )
        .unwrap();
        for (name, want) in [
            ("notes.md", ["glow", "%s"]),
            ("docs/README.MD", ["glow", "%s"]),
            ("a.py", ["bat", "%s"]),
            ("md/x.txt", ["less", "%s"]),
        ] {
            assert_eq!(c.open_cmd_for(name), Some(&strings(&want)[..]), "{name}");
        }
        let c = parse("vedi.conf", "open_cmd *.md mdcat %s\n").unwrap();
        assert_eq!(c.open_cmd_for("a.txt"), None);
    }

    /// The pattern syntax open_cmd takes: *, ?, [a-c] and [!a-c], with
    /// [*] for a literal star; case is ignored.
    #[test]
    fn glob() {
        for (pattern, name, want) in [
            ("*.md", "a.md", true),
            ("*.md", "a.mdx", false),
            ("*.md", "A.MD", true),
            ("*é.txt", "CAFÉ.TXT", true),
            ("*É.TXT", "café.txt", true),
            ("a?c", "abc", true),
            ("a?c", "ac", false),
            ("[a-c]x", "bx", true),
            ("[!a-c]x", "dx", true),
            ("[!a-c]x", "bx", false),
            ("[*].md", "*.md", true),
            ("[*].md", "a.md", false),
            ("a*b*c", "axxbyyc", true),
        ] {
            let c = parse("vedi.conf", &format!("open_cmd {pattern} cat %s\n")).unwrap();
            assert_eq!(c.open_cmd_for(name).is_some(), want, "{pattern} {name}");
        }
        for pattern in ["[md", "x[!]"] {
            assert_eq!(
                parse_err(&format!("open_cmd {pattern} cat %s\n")),
                format!("vedi.conf:1: open_cmd: bad pattern {pattern}")
            );
        }
    }

    #[test]
    fn one_page_rows_below() {
        let tests = [
            ("", 1),
            ("one_page_rows_below 2\n", 2),
            ("one_page_rows_below 2\none_page_rows_below 3\n", 3),
        ];
        for (src, want) in tests {
            assert_eq!(
                parse("vedi.conf", src).map(|c| c.settings.one_page_rows_below),
                Ok(want),
                "{src:?}"
            );
        }
        for arg in ["", " two", " 0", " -1", " 1 2"] {
            let src = format!("one_page_rows_below{arg}\n");
            assert_eq!(
                parse_err(&src),
                "vedi.conf:1: one_page_rows_below takes a positive number",
                "{src:?}"
            );
        }
    }

    #[test]
    fn tab_width() {
        let tests = [
            ("", 0),
            ("tab_width 4\n", 4),
            ("tab_width 8\n", 8),
            ("tab_width 2\ntab_width 4\n", 4),
            ("tab_width 4\nmap x quit\n", 4),
        ];
        for (src, want) in tests {
            assert_eq!(
                parse("vedi.conf", src).map(|c| c.settings.tab_width),
                Ok(want),
                "{src:?}"
            );
        }
        for arg in ["", " four", " 0", " -4", " 4 8"] {
            let src = format!("tab_width{arg}\n");
            assert_eq!(
                parse_err(&src),
                "vedi.conf:1: tab_width takes a positive number",
                "{src:?}"
            );
        }
    }

    fn doc(name: &str) -> String {
        let path = format!("{}/../docs/{name}", env!("CARGO_MANIFEST_DIR"));
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The first fenced code block after each "## " heading, by heading.
    fn blocks(md: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut heading = "";
        let mut body: Option<String> = None;
        for line in md.split('\n') {
            if let Some(h) = line.strip_prefix("## ") {
                heading = h;
            } else if line.starts_with("```") {
                match body.take() {
                    Some(b) if !out.iter().any(|(h, _)| h == heading) => {
                        out.push((heading.to_string(), b))
                    }
                    Some(_) => {}
                    None => body = Some(String::new()),
                }
            } else if let Some(b) = &mut body {
                b.push_str(line);
                b.push('\n');
            }
        }
        out
    }

    /// docs/configuration.md stays true: its Defaults block is the
    /// embedded file, and its examples parse.
    #[test]
    fn configuration_doc() {
        let b = blocks(&doc("configuration.md"));
        let block = |name: &str| {
            let found = b.iter().find(|(h, _)| h == name);
            found
                .unwrap_or_else(|| panic!("no {name} block"))
                .1
                .as_str()
        };
        assert_eq!(
            block("Defaults"),
            DEFAULTS,
            "the Defaults block differs from defaults.conf"
        );
        for name in ["vim", "emacs"] {
            let c = parse(name, block(name)).unwrap_or_else(|e| panic!("{name} example: {e}"));
            assert!(!c.keys.is_empty(), "{name} example binds nothing");
        }
    }

    fn rows(secs: &[Section]) -> Vec<&Row> {
        secs.iter().flat_map(|s| &s.rows).collect()
    }

    #[test]
    fn help_default() {
        let secs = default_keymap(false).help();
        let top = &secs[0];
        assert_eq!(top.name, "");
        assert_eq!(top.rows.len(), 5);
        assert_eq!(top.rows[0].doc, "Show the key bindings");
        assert_eq!(top.rows[0].keys, ["h"]);
        let moving = &secs[1];
        assert_eq!(moving.name, "Moving");
        let want = [
            (
                &[
                    "Up", "Down", "Left", "Right", "Home", "End", "j", "k", "0", "$",
                ][..],
                "Move the cursor",
            ),
            (&["PgDn", "Space", "f", "Ctrl+f", "Ctrl+v"], "Down a page"),
            (&["PgUp", "b", "Ctrl+b", "Alt+v"], "Up a page"),
        ];
        for (i, (keys, doc)) in want.into_iter().enumerate() {
            assert_eq!(moving.rows[i].keys, keys, "row {i}");
            assert_eq!(moving.rows[i].doc, doc, "row {i}");
        }
        assert_eq!(
            moving.rows.last().unwrap().doc,
            "Command prompt; N goes to line N"
        );
        let names: Vec<&str> = secs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "",
                "Moving",
                "Selecting",
                "Mouse",
                "Copying",
                "Searching",
                "Prompts"
            ]
        );
        let all = rows(&secs);
        assert!(
            all.iter().all(|r| r.doc != "Extend by half a page"),
            "a row with no keys is shown"
        );
        let last = all.last().unwrap();
        assert_eq!(last.keys, ["Esc", "Ctrl+g", "Ctrl+c"]);
        assert_eq!(last.doc, "Cancel");
        let mac = default_keymap(true).help();
        assert_eq!(
            mac[1].rows[0].keys,
            [
                "Up",
                "Down",
                "Left",
                "Right",
                "Home",
                "End",
                "j",
                "k",
                "0",
                "$",
                "Cmd+Left",
                "Cmd+Right"
            ]
        );
    }

    #[test]
    fn help_applied() {
        let m = default_keymap(false).apply(&[
            bind("h", "left"),
            bind("Alt+h", "help"),
            bind("w", "none"),
        ]);
        let secs = m.help();
        assert_eq!(
            secs[1].rows[0].keys,
            [
                "Up", "Down", "Left", "Right", "Home", "End", "j", "k", "0", "$", "h"
            ]
        );
        assert!(
            rows(&secs).iter().all(|r| r.doc != "Toggle wrap / nowrap"),
            "unbound action still has a row"
        );
        assert_eq!(secs[0].rows[0].keys, ["Alt+h"]);
    }

    /// A section none of whose rows has a key is left out.
    #[test]
    fn help_empty_section() {
        let m = default_keymap(false).apply(&[
            bind("Ctrl+c", "none"),
            bind("y", "none"),
            bind("Enter", "none"),
        ]);
        assert!(
            m.help().iter().all(|s| s.name != "Copying"),
            "Copying is shown with nothing in it"
        );
    }

    /// The table in docs/keys.md and the help screen are the same list:
    /// each row of the table is a row, in order, with the macOS keys.
    #[test]
    fn help_matches_keys_doc() {
        let secs = default_keymap(true).help();
        let all = rows(&secs);
        let mut got = Vec::new();
        let mut in_keys = false;
        let md = doc("keys.md");
        for line in md.split('\n') {
            if line.starts_with('#') {
                in_keys = line == "# Keys";
                continue;
            }
            if !in_keys || !line.starts_with('|') {
                continue;
            }
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let [keys, doc] = cells[..] else {
                panic!("docs/keys.md row {line:?}: want two cells");
            };
            if keys != "Key" && keys != "---" {
                got.push((keys, doc));
            }
        }
        assert_eq!(got.len(), all.len(), "docs/keys.md rows, help rows");
        for (i, (r, (keys, doc))) in all.iter().zip(got).enumerate() {
            // Fixed rows hold a comma themselves, so compare the joined text.
            assert_eq!(
                (r.keys.join(", ").as_str(), r.doc.as_str()),
                (keys, doc),
                "row {i}"
            );
        }
    }
}
