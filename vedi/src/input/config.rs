//! vedi.conf: one directive per line, kitty style.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use glob::Pattern;

use crate::input::action::Command;
use crate::input::key::Key;
use crate::input::keymap::{Binding, DEFAULTS, Keymap};
use crate::input::setting::{self, Settings};
use crate::sys::words;
use crate::view::style::Element;

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

/// An open_cmd line: the files `names` takes are read by running
/// `argv`, %s the name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OpenCmd {
    pub names: Names,
    pub argv: Vec<String>,
}

/// The files an open_cmd line is for, by base name, both sides
/// lowercased.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Names {
    /// The fallback: every file, after the pattern lines matching it.
    Rest,
    Matching(Pattern),
    /// `--no-fallback`: a file it matches skips the `Rest` lines.
    NoFallback(Pattern),
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
    /// The commands `name` can be read through, the one it is read
    /// through first: the pattern lines matching its base name,
    /// ignoring case, then the plain lines unless a `--no-fallback` line
    /// matched, each last line first.
    pub fn open_cmds_for(&self, name: &str) -> Vec<&[String]> {
        // Both sides lowercased: the glob crate folds ASCII only.
        let base = Path::new(name)
            .file_name()
            .map_or(name.into(), |b| b.to_string_lossy())
            .to_lowercase();
        let (mut matched, mut plain, mut fallback) = (Vec::new(), Vec::new(), true);
        for o in self.open_cmds.iter().rev() {
            match &o.names {
                Names::Rest => plain.push(&o.argv[..]),
                Names::Matching(p) if p.matches(&base) => matched.push(&o.argv[..]),
                Names::NoFallback(p) if p.matches(&base) => {
                    matched.push(&o.argv[..]);
                    fallback = false;
                }
                Names::Matching(_) | Names::NoFallback(_) => {}
            }
        }
        if fallback {
            matched.append(&mut plain);
        }
        matched
    }

    /// The bindings in effect: the defaults, or none after
    /// clear_all_shortcuts, with the map lines laid over.
    pub fn keymap(&self) -> Keymap {
        if self.clear {
            return Keymap::new(self.keys.clone());
        }
        default_keymap().apply(&self.keys)
    }
}

/// The built-in key map.
pub fn default_keymap() -> Keymap {
    Keymap::new(
        parse("defaults.conf", DEFAULTS)
            .expect("defaults.conf parses")
            .keys,
    )
}

/// Applies `src` line by line.
pub fn parse(name: &str, src: &str) -> Result<Config, ConfigError> {
    let mut c = Config::default();
    apply(name, src, None, &mut c, &mut Includes::default())?;
    Ok(c)
}

/// What include lines need beyond the line.
#[derive(Default)]
struct Includes {
    /// What a leading ~ stands for.
    home: Option<PathBuf>,
    /// The files being applied, which an include of one would loop
    /// through.
    open: Vec<PathBuf>,
}

/// Applies `src`, the file `name`, to `c` line by line. An include line
/// applies the file it names, after a leading ~ or relative to `dir`;
/// with no `dir` there is no file to be relative to, and it is refused.
fn apply(
    name: &str,
    src: &str,
    dir: Option<&Path>,
    c: &mut Config,
    inc: &mut Includes,
) -> Result<(), ConfigError> {
    for (i, line) in src.split('\n').enumerate() {
        let at = |msg| ConfigError {
            name: name.to_string(),
            line: i + 1,
            msg,
        };
        let line = line.trim();
        let Some(file) = line
            .strip_prefix("include")
            .filter(|r| r.is_empty() || r.starts_with(char::is_whitespace))
        else {
            parse_line(line, c).map_err(at)?;
            continue;
        };
        let file = file.trim();
        if file.is_empty() {
            return Err(at("include takes a file name".into()));
        }
        let Some(dir) = dir else {
            return Err(at("include needs a config file to be relative to".into()));
        };
        let path = dir.join(words::expand(inc.home.as_deref(), file));
        let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if inc.open.contains(&real) {
            return Err(at(format!("include {file}: includes itself")));
        }
        let src = fs::read(&path).map_err(|e| at(format!("include {file}: {e}")))?;
        apply_file(&path, &src, c, inc)?;
    }
    Ok(())
}

/// Applies the file at `path`, which holds `src`.
fn apply_file(
    path: &Path,
    src: &[u8],
    c: &mut Config,
    inc: &mut Includes,
) -> Result<(), ConfigError> {
    inc.open
        .push(fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    let name = path.display().to_string();
    let dir = path.parent().unwrap_or(Path::new(""));
    apply(&name, &String::from_utf8_lossy(src), Some(dir), c, inc)?;
    inc.open.pop();
    Ok(())
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
    if let Some(v) = setting::verb(verb)
        && let Some(f) = &v.field
    {
        v.check(arg)?;
        (f.set)(&mut c.settings, arg).expect("checked");
        return Ok(Some(verb));
    }
    match verb {
        "map" => {
            if f.len() != 3 && !(f.len() > 3 && f[2] == "cycle") {
                return Err("map takes a key and an action".to_string());
            }
            let key: Key = f[1].parse().map_err(|e| format!("{e}"))?;
            let cmd = Command::parse_binding(&f[2..].join(" ")).map_err(|e| format!("{e}"))?;
            c.keys.push(Binding { key, cmd });
        }
        "color" => {
            let [_, name, _, ..] = f[..] else {
                return Err("color takes an element and a style".to_string());
            };
            let e: Element = name
                .parse()
                .map_err(|()| format!("unknown element {name:?}"))?;
            c.settings.theme[e] = f[2..].join(" ").parse()?;
        }
        "clear_all_shortcuts" => {
            if f.len() != 1 {
                return Err("clear_all_shortcuts takes nothing".to_string());
            }
            c.keys.clear();
            c.clear = true;
        }
        "clipboard_cmd" => {
            if rest.is_empty() {
                return Err("clipboard_cmd takes a command".to_string());
            }
            c.clipboard_cmd = Some(rest.to_string());
        }
        "open_cmd" => c.open_cmds.push(parse_open_cmd(rest)?),
        _ => return Err(format!("unknown verb {verb:?}")),
    }
    Ok(Some(verb))
}

/// What follows open_cmd: `--no-fallback` and a pattern, or a pattern, when
/// the first word has a glob character, then the command.
fn parse_open_cmd(s: &str) -> Result<OpenCmd, String> {
    let mut argv = words::split(s).map_err(|e| format!("open_cmd: {e}"))?;
    let no_fallback = argv.first().is_some_and(|w| w == "--no-fallback");
    if no_fallback {
        argv.remove(0);
    }
    let mut names = Names::Rest;
    if argv.first().is_some_and(|w| w.contains(['*', '?', '['])) {
        let first = argv.remove(0);
        let p = Pattern::new(&first.to_lowercase())
            .map_err(|_| format!("open_cmd: bad pattern {first}"))?;
        names = if no_fallback {
            Names::NoFallback(p)
        } else {
            Names::Matching(p)
        };
    } else if no_fallback {
        return Err("open_cmd: --no-fallback takes a pattern".to_string());
    }
    if argv.is_empty() {
        return Err("open_cmd takes a command".to_string());
    }
    Ok(OpenCmd { names, argv })
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
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty());
    load_in(path, home.map(PathBuf::from))
}

/// `load`, with `home` for a leading ~.
fn load_in(path: &Path, home: Option<PathBuf>) -> Result<Config, LoadError> {
    if path.as_os_str().is_empty() {
        return Ok(Config::default());
    }
    let src = fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => LoadError::NotFound(e),
        _ => LoadError::Io(e),
    })?;
    let mut c = Config::default();
    let mut inc = Includes {
        home,
        open: Vec::new(),
    };
    apply_file(path, &src, &mut c, &mut inc).map_err(LoadError::Parse)?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::help::{Row, Section};
    use crate::input::key::{KeyCode, Mods};
    use crate::text::ansi::ViewStyle;
    use crate::view::layout::{Mode, WrapStyle};
    use crate::view::style::{Color, Style, Theme};

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
        let src = "# vim\n\nmap h left\n\tmap  L\tselect_right \nmap ? none\r\nmap Alt+w cycle wrap_style\nmap T cycle  tab_width 04 8\n";
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
                Binding {
                    key: Key::new(KeyCode::Char('T'), Mods::NONE),
                    cmd: cmd("cycle tab_width 4 8")
                },
            ]
        );
        assert_eq!(parse("empty", ""), Ok(Config::default()));
        let cycle = format!("vedi.conf:1: {}", "cycle".parse::<Command>().unwrap_err());
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
            ("map h cycle\n", cycle.as_str()),
            ("map h cycle foo\n", cycle.as_str()),
            (
                "map h cycle tab_width\n",
                "vedi.conf:1: cycle tab_width takes two or more values",
            ),
            (
                "map h cycle wrap yes\n",
                "vedi.conf:1: cycle wrap takes two or more values",
            ),
            (
                "map h cycle wrap yes maybe\n",
                "vedi.conf:1: wrap takes yes or no",
            ),
            (
                "map h cycle open_cmd bat\n",
                "vedi.conf:1: cycle open_cmd takes no values",
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
        assert_eq!(c.keymap(), Keymap::new(want));
        let c = parse("vedi.conf", "clear_all_shortcuts\n").unwrap();
        assert_eq!(c.keymap().bindings(), []);
        let c = parse("vedi.conf", "map x quit\n").unwrap();
        assert_eq!(c.keymap(), default_keymap().apply(&c.keys));
    }

    #[test]
    fn defaults_bind_each_key_once() {
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
            ("up", Key::new(Up, Mods::NONE), "up"),
            ("shift up extends", Key::new(Up, Mods::SHIFT), "select_up"),
            (
                "ctrl on a named key",
                Key::new(Left, Mods::CTRL),
                "word_left",
            ),
            ("alt on a rune", Key::new(ch('b'), Mods::ALT), "word_left"),
            ("ctrl f", Key::new(ch('f'), Mods::CTRL), "page_down"),
            (
                "ctrl f as a control character",
                Key::new(ch('\x06'), Mods::NONE),
                "page_down",
            ),
            (
                "shift on a character",
                Key::new(ch('<'), Mods::SHIFT),
                "first",
            ),
            ("G never extends", Key::new(ch('G'), Mods::SHIFT), "last"),
            ("enter", Key::new(Enter, Mods::NONE), "down"),
            ("unbound rune", Key::new(ch('z'), Mods::NONE), "none"),
            (
                "unlisted modifier is unbound",
                Key::new(Home, Mods::ALT),
                "none",
            ),
            ("cmd c", Key::new(ch('c'), Mods::CMD), "copy"),
            (
                "cmd shift G as a capital",
                Key::new(ch('G'), Mods::CMD | Mods::SHIFT),
                "search_prev",
            ),
            (
                "cmd shift up extends",
                Key::new(Up, Mods::CMD | Mods::SHIFT),
                "select_first",
            ),
            ("cmd q is unbound", Key::new(ch('q'), Mods::CMD), "none"),
        ];
        for (name, k, want) in tests {
            assert_eq!(default_keymap().lookup(k), cmd(want), "{name}");
        }
    }

    #[test]
    fn lookup_applied() {
        let m = default_keymap().apply(&[
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
            default_keymap().apply(&c.keys).lookup(key("h")),
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

    #[test]
    fn include() {
        let dir =
            TempDir(std::env::temp_dir().join(format!("vedi-include-{}", std::process::id())));
        fs::create_dir_all(dir.0.join("themes")).unwrap();
        let path = dir.0.join("vedi.conf");
        let theme = dir.0.join("themes/t.conf");
        fs::write(&theme, "color match red\ncolor hint blue\n").unwrap();
        fs::write(
            &path,
            "map h left\ninclude themes/t.conf\ncolor hint green\n",
        )
        .unwrap();
        let c = load(&path).unwrap();
        let t = c.settings.theme;
        assert_eq!(t[Element::Match], "red".parse().unwrap());
        assert_eq!(
            t[Element::Hint],
            "green".parse().unwrap(),
            "later lines win"
        );
        assert_eq!(c.keys.len(), 1);
        fs::write(&path, "include ~/themes/t.conf\n").unwrap();
        let c = load_in(&path, Some(dir.0.clone())).unwrap();
        let blue = "blue".parse().unwrap();
        assert_eq!(c.settings.theme[Element::Hint], blue, "~ is home");

        let err = |src: &str| {
            fs::write(&path, src).unwrap();
            load(&path).unwrap_err().to_string()
        };
        let (conf, t) = (path.display(), theme.display());
        fs::write(&theme, "color match purple\n").unwrap();
        assert_eq!(
            err("include themes/t.conf\n"),
            format!(r#"{t}:1: unknown color "purple""#),
            "an included line's error names its file"
        );
        fs::write(&theme, "include ../vedi.conf\n").unwrap();
        assert_eq!(
            err("\ninclude themes/t.conf\n"),
            format!("{t}:1: include ../vedi.conf: includes itself"),
        );
        assert_eq!(
            err("include\n"),
            format!("{conf}:1: include takes a file name")
        );
        assert!(err("include none.conf\n").starts_with(&format!("{conf}:1: include none.conf: ")));
        assert_eq!(
            parse("config", "include t.conf").unwrap_err().to_string(),
            "config:1: include needs a config file to be relative to"
        );
    }

    #[test]
    fn color() {
        let c = parse(
            "vedi.conf",
            "color match on blue bold\ncolor guide default\n",
        )
        .unwrap();
        let t = c.settings.theme;
        assert_eq!(
            t[Element::Match],
            Style::default().bg(Color::Palette(4)).bold()
        );
        assert_eq!(t[Element::Guide], Style::default());
        assert_eq!(t[Element::Selection], Theme::default()[Element::Selection]);
        for (src, msg) in [
            ("color", "color takes an element and a style"),
            ("color match", "color takes an element and a style"),
            ("color matches red", r#"unknown element "matches""#),
            ("color match on", "on takes a color"),
        ] {
            assert_eq!(parse_err(src), format!("vedi.conf:1: {msg}"), "{src}");
        }
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
    fn end_markers() {
        check_yes_no("end_markers", |s| s.end_markers);
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
            names: pattern.map_or(Names::Rest, |p| Names::Matching(Pattern::new(p).unwrap())),
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
            (
                "open_cmd --no-fallback *.json jq %s\n",
                vec![OpenCmd {
                    names: Names::NoFallback(Pattern::new("*.json").unwrap()),
                    argv: strings(&["jq", "%s"]),
                }],
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
            (
                "open_cmd --no-fallback jq %s\n",
                "vedi.conf:1: open_cmd: --no-fallback takes a pattern",
            ),
            (
                "open_cmd --no-fallback *.json\n",
                "vedi.conf:1: open_cmd takes a command",
            ),
        ] {
            assert_eq!(parse_err(src), want, "{src:?}");
        }
    }

    /// A file is read through the last pattern line matching its base
    /// name, ignoring case, else the last plain line, else as it is; O
    /// steps on to the other matching lines, then the plain ones unless
    /// a --no-fallback line matched.
    #[test]
    fn open_cmds_for() {
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
            ("notes.md", &["glow", "mdcat", "less", "cat"][..]),
            ("docs/README.MD", &["glow", "mdcat", "less", "cat"]),
            ("a.py", &["bat", "less", "cat"]),
            ("md/x.txt", &["less", "cat"]),
        ] {
            let got: Vec<&str> = c.open_cmds_for(name).iter().map(|a| &a[0][..]).collect();
            assert_eq!(got, want, "{name}");
        }
        let c = parse("vedi.conf", "open_cmd *.md mdcat %s\n").unwrap();
        assert!(c.open_cmds_for("a.txt").is_empty());
        let c = parse(
            "vedi.conf",
            "
open_cmd *.json bat %s
open_cmd --no-fallback *.json jq %s
open_cmd cat %s
",
        )
        .unwrap();
        for (name, want) in [("a.json", &["jq", "bat"][..]), ("a.md", &["cat"])] {
            let got: Vec<&str> = c.open_cmds_for(name).iter().map(|a| &a[0][..]).collect();
            assert_eq!(got, want, "{name}");
        }
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
            assert_eq!(!c.open_cmds_for(name).is_empty(), want, "{pattern} {name}");
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
        for arg in ["", " two", " 0", " -1", " 1 2", " 65536"] {
            let src = format!("one_page_rows_below{arg}\n");
            assert_eq!(
                parse_err(&src),
                "vedi.conf:1: one_page_rows_below takes a number from 1 to 65535",
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
        for arg in ["", " four", " 0", " -4", " 4 8", " 65536"] {
            let src = format!("tab_width{arg}\n");
            assert_eq!(
                parse_err(&src),
                "vedi.conf:1: tab_width takes a number from 1 to 65535",
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

    /// The Colors table names each element, with its default.
    #[test]
    fn colors_doc() {
        let md = doc("configuration.md");
        let mut named = Vec::new();
        for line in md.lines().filter(|l| l.starts_with("| `")) {
            let cells: Vec<&str> = line
                .split('|')
                .map(|c| c.trim().trim_matches('`'))
                .collect();
            let Ok(e) = cells[1].parse::<Element>() else {
                continue;
            };
            let style: Style = cells[2].parse().unwrap_or_else(|m| panic!("{line}: {m}"));
            assert_eq!(style, Theme::default()[e], "{line}");
            named.push(e);
        }
        assert_eq!(named, Element::ALL);
    }

    /// Each theme shipped loads, and names every element.
    #[test]
    fn themes() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
        let mut n = 0;
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            load(&path).unwrap_or_else(|e| panic!("{e}"));
            let src = fs::read_to_string(&path).unwrap();
            for e in Element::ALL {
                let line = format!("color {} ", e.name());
                assert!(src.contains(&line), "{}: no {}", path.display(), e.name());
            }
            n += 1;
        }
        assert!(n > 0, "no themes in {}", dir.display());
    }

    fn rows(secs: &[Section]) -> Vec<&Row> {
        secs.iter().flat_map(|s| &s.rows).collect()
    }

    #[test]
    fn help_default() {
        let secs = default_keymap().help();
        let top = &secs[0];
        assert_eq!(top.name, "");
        assert_eq!(top.rows.len(), 7);
        assert_eq!(top.rows[0].doc, "Show the key bindings");
        assert_eq!(top.rows[0].keys, ["h"]);
        let moving = &secs[1];
        assert_eq!(moving.name, "Moving");
        let want = [
            (
                &[
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
                    "Enter",
                    "Cmd+Left",
                    "Cmd+Right",
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
    }

    #[test]
    fn help_applied() {
        let m =
            default_keymap().apply(&[bind("h", "left"), bind("Alt+h", "help"), bind("w", "none")]);
        let secs = m.help();
        assert_eq!(
            secs[1].rows[0].keys,
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
                "Enter",
                "Cmd+Left",
                "Cmd+Right",
                "h"
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
        let m = default_keymap().apply(&[
            bind("Ctrl+c", "none"),
            bind("Cmd+c", "none"),
            bind("y", "none"),
        ]);
        assert!(
            m.help().iter().all(|s| s.name != "Copying"),
            "Copying is shown with nothing in it"
        );
    }

    /// The table in docs/keys.md and the help screen are the same list:
    /// each row of the table is a row, in order.
    #[test]
    fn help_matches_keys_doc() {
        let secs = default_keymap().help();
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
