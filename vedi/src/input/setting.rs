use crate::text::ansi::ViewStyle;
use crate::view::layout::{Mode, WrapStyle};

/// What a config line, a flag or the : prompt sets, as the pager holds
/// it. `Default` is the value with none of them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Settings {
    pub mode: Mode,
    pub wrap_style: WrapStyle,
    /// Cells per tab stop; 0 for the default.
    pub tab_width: usize,
    /// Mark text off the sides with < and >, a wrapped row with \.
    pub edge_markers: bool,
    /// A row names each input, when there are several.
    pub file_separators: bool,
    /// A `Changed` event reads the file again.
    pub auto_reload: bool,
    /// Off, the status line shows only when it has something to say.
    pub status_line: bool,
    pub view_style: ViewStyle,
    /// Rows -F leaves below the text.
    pub one_page_rows_below: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            mode: Mode::Wrap,
            wrap_style: WrapStyle::Char,
            tab_width: 0,
            edge_markers: false,
            file_separators: true,
            auto_reload: true,
            status_line: true,
            view_style: ViewStyle::Color,
            one_page_rows_below: 1,
        }
    }
}

impl Settings {
    /// The value of a setting cycle takes, as a config file spells it.
    pub fn value(&self, name: &str) -> String {
        match name {
            "wrap" => yes_no(self.mode == Mode::Wrap),
            "wrap_style" => self.wrap_style.to_string(),
            "edge_markers" => yes_no(self.edge_markers),
            "file_separators" => yes_no(self.file_separators),
            "auto_reload" => yes_no(self.auto_reload),
            "status_line" => yes_no(self.status_line),
            "view_style" => self.view_style.to_string(),
            _ => String::new(),
        }
    }
}

fn yes_no(b: bool) -> String {
    if b { "yes" } else { "no" }.to_string()
}

/// A config verb, and what the flags, the : prompt and cycle need to
/// know of it.
pub struct Verb {
    pub name: &'static str,
    /// Its values, which cycle steps through and Tab fills in; empty
    /// for a verb that takes anything else, or nothing.
    pub values: &'static [&'static str],
    /// The status line shows it, so cycle needn't say it: wrap,
    /// wrap_style and view_style are on it; status_line is it.
    pub shown: bool,
    /// The : prompt takes it: not clear_all_shortcuts, which would
    /// unbind : and q; open_cmd, the input being open; nor
    /// one_page_rows_below, which only -F reads.
    pub prompt: bool,
    /// A flag spells it, --name with - for _, and --no-name for a
    /// yes/no: not map or clear_all_shortcuts, nor clipboard_cmd and
    /// open_cmd, whose --no- forms and --open-cmd say what no config
    /// line can.
    pub flag: bool,
}

const YES_NO: &[&str] = &["yes", "no"];

/// Every verb `config::parse_line` takes, in the order cycle and the
/// error for a bad cycle list them.
pub const VERBS: &[Verb] = &[
    Verb {
        name: "wrap",
        values: YES_NO,
        shown: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "wrap_style",
        values: &["char", "word"],
        shown: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "tab_width",
        values: &[],
        shown: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "edge_markers",
        values: YES_NO,
        shown: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "file_separators",
        values: YES_NO,
        shown: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "auto_reload",
        values: YES_NO,
        shown: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "status_line",
        values: YES_NO,
        shown: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "view_style",
        values: &["color", "plain", "raw"],
        shown: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "clipboard_cmd",
        values: &[],
        shown: false,
        prompt: true,
        flag: false,
    },
    Verb {
        name: "map",
        values: &[],
        shown: false,
        prompt: true,
        flag: false,
    },
    Verb {
        name: "clear_all_shortcuts",
        values: &[],
        shown: false,
        prompt: false,
        flag: false,
    },
    Verb {
        name: "open_cmd",
        values: &[],
        shown: false,
        prompt: false,
        flag: false,
    },
    Verb {
        name: "one_page_rows_below",
        values: &[],
        shown: false,
        prompt: false,
        flag: true,
    },
];

/// What cycle takes, in order: the verbs with values.
pub fn names() -> impl Iterator<Item = &'static str> {
    VERBS
        .iter()
        .filter(|v| !v.values.is_empty())
        .map(|v| v.name)
}

/// The verbs the : prompt takes, in order.
pub fn at_prompt() -> impl Iterator<Item = &'static str> {
    VERBS.iter().filter(|v| v.prompt).map(|v| v.name)
}

/// `names` as an error lists them: a, b or c.
pub fn or_list(names: &[&str]) -> String {
    let (last, rest) = names.split_last().expect("names");
    format!("{} or {last}", rest.join(", "))
}

/// A setting cycle takes.
pub fn lookup(name: &str) -> Option<&'static Verb> {
    VERBS
        .iter()
        .find(|v| v.name == name && !v.values.is_empty())
}

/// The verb behind flag `name`, `--X` or `--no-X` for verb x, and the
/// value the flag alone means: yes or no, or `None` for one that takes
/// a value. `None` for any other arg.
pub fn flag_verb(name: &str) -> Option<(&'static Verb, Option<&'static str>)> {
    let (rest, no) = match name.strip_prefix("--no-") {
        Some(rest) => (rest, true),
        None => (name.strip_prefix("--")?, false),
    };
    let v = VERBS
        .iter()
        .find(|v| v.flag && v.name.replace('_', "-") == rest)?;
    match (v.yes_no(), no) {
        (true, true) => Some((v, Some("no"))),
        (true, false) => Some((v, Some("yes"))),
        (false, false) => Some((v, None)),
        (false, true) => None,
    }
}

impl Verb {
    pub fn yes_no(&self) -> bool {
        self.values == YES_NO
    }

    /// The value after `v`: the first after the last, or after a value
    /// not in the list.
    pub fn next(&self, v: &str) -> &'static str {
        match self.values.iter().position(|&x| x == v) {
            Some(i) => self.values[(i + 1) % self.values.len()],
            None => self.values[0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// next steps through a setting's values and comes back round; an
    /// unknown value starts over.
    #[test]
    fn setting_next() {
        let s = lookup("wrap_style").unwrap();
        assert_eq!(s.values, ["char", "word"]);
        for (cur, next) in [("char", "word"), ("word", "char"), ("", "char")] {
            assert_eq!(s.next(cur), next, "{cur:?}");
        }
        assert!(lookup("tab_width").is_none());
    }

    /// The prompt takes every verb but the three a running pager
    /// cannot use.
    #[test]
    fn prompt_verbs() {
        let out: Vec<&str> = VERBS.iter().filter(|v| !v.prompt).map(|v| v.name).collect();
        assert_eq!(
            out,
            ["clear_all_shortcuts", "open_cmd", "one_page_rows_below"]
        );
        assert_eq!(at_prompt().count() + out.len(), VERBS.len());
    }

    /// A flag sets each setting and nothing else; --no- is for yes/no.
    #[test]
    fn flag_verbs() {
        let flags: Vec<&str> = VERBS.iter().filter(|v| v.flag).map(|v| v.name).collect();
        assert_eq!(
            flags,
            [
                "wrap",
                "wrap_style",
                "tab_width",
                "edge_markers",
                "file_separators",
                "auto_reload",
                "status_line",
                "view_style",
                "one_page_rows_below",
            ]
        );
        let name = |f: &str| flag_verb(f).map(|(v, val)| (v.name, val));
        assert_eq!(name("--wrap"), Some(("wrap", Some("yes"))));
        assert_eq!(name("--no-wrap"), Some(("wrap", Some("no"))));
        assert_eq!(name("--tab-width"), Some(("tab_width", None)));
        assert_eq!(
            name("--one-page-rows-below"),
            Some(("one_page_rows_below", None))
        );
        for f in [
            "--no-tab-width",
            "--wrap_style",
            "--map",
            "--clear-all-shortcuts",
            "--open-cmd",
            "--clipboard-cmd",
            "--no-open-cmd",
            "-S",
            "wrap",
        ] {
            assert_eq!(name(f), None, "{f}");
        }
    }
}
