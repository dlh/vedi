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

fn yes_no(b: bool) -> String {
    if b { "yes" } else { "no" }.to_string()
}

/// A number from 1 to 65535, or what the verb takes: a terminal has at
/// most u16::MAX cells a row, and a wider tab overflows the layout's
/// sums.
fn positive(arg: &str) -> Result<usize, &'static str> {
    match arg.parse::<u16>() {
        Ok(n) if n > 0 => Ok(usize::from(n)),
        _ => Err("a number from 1 to 65535"),
    }
}

/// The setting a verb sets, when it is one.
pub struct Field {
    /// Its value as a config file spells it.
    pub get: fn(&Settings) -> String,
    /// Sets it from the line's one argument, checked against `values`
    /// first when the verb has them. The error says what the verb takes.
    pub set: fn(&mut Settings, &str) -> Result<(), &'static str>,
}

/// How cycle steps a verb.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Cycle {
    /// Not at all: it takes a command, a number, or nothing.
    No,
    /// Through these, which are also all the verb takes.
    Values(&'static [&'static str]),
    /// Off and on: open_cmd, between its commands and none.
    Toggle,
}

/// A config verb, and what the flags, the : prompt and cycle need to
/// know of it.
pub struct Verb {
    pub name: &'static str,
    pub cycle: Cycle,
    /// `None` for a verb that is no setting: map, clipboard_cmd and
    /// their kind.
    pub field: Option<Field>,
    /// The status line's summary row shows its value, so cycle needn't
    /// say it: wrap, wrap_style and view_style are on it; status_line
    /// is it.
    pub summarized: bool,
    /// The : prompt takes it: not clipboard_cmd, which a paste could
    /// set to any command for y to run; clear_all_shortcuts, which
    /// would unbind : and q; open_cmd, the input being open; nor
    /// one_page_rows_below, which only -F reads.
    pub prompt: bool,
    /// A flag spells it, --name with - for _, and --no-name for a
    /// yes/no: not map or clear_all_shortcuts, nor clipboard_cmd and
    /// open_cmd, whose --no- forms and --open-cmd say what no config
    /// line can.
    pub flag: bool,
}

const YES_NO: Cycle = Cycle::Values(&["yes", "no"]);

/// Every verb `config::parse_line` takes, in the order cycle and the
/// error for a bad cycle list them.
pub const VERBS: &[Verb] = &[
    Verb {
        name: "wrap",
        cycle: YES_NO,
        field: Some(Field {
            get: |s| yes_no(s.mode == Mode::Wrap),
            set: |s, a| {
                s.mode = if a == "yes" { Mode::Wrap } else { Mode::NoWrap };
                Ok(())
            },
        }),
        summarized: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "wrap_style",
        cycle: Cycle::Values(&["char", "word"]),
        field: Some(Field {
            get: |s| s.wrap_style.to_string(),
            set: |s, a| {
                s.wrap_style = a.parse().expect("checked");
                Ok(())
            },
        }),
        summarized: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "tab_width",
        cycle: Cycle::No,
        field: Some(Field {
            get: |s| s.tab_width.to_string(),
            set: |s, a| {
                s.tab_width = positive(a)?;
                Ok(())
            },
        }),
        summarized: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "edge_markers",
        cycle: YES_NO,
        field: Some(Field {
            get: |s| yes_no(s.edge_markers),
            set: |s, a| {
                s.edge_markers = a == "yes";
                Ok(())
            },
        }),
        summarized: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "file_separators",
        cycle: YES_NO,
        field: Some(Field {
            get: |s| yes_no(s.file_separators),
            set: |s, a| {
                s.file_separators = a == "yes";
                Ok(())
            },
        }),
        summarized: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "auto_reload",
        cycle: YES_NO,
        field: Some(Field {
            get: |s| yes_no(s.auto_reload),
            set: |s, a| {
                s.auto_reload = a == "yes";
                Ok(())
            },
        }),
        summarized: false,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "status_line",
        cycle: YES_NO,
        field: Some(Field {
            get: |s| yes_no(s.status_line),
            set: |s, a| {
                s.status_line = a == "yes";
                Ok(())
            },
        }),
        summarized: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "view_style",
        cycle: Cycle::Values(&["color", "plain", "raw"]),
        field: Some(Field {
            get: |s| s.view_style.to_string(),
            set: |s, a| {
                s.view_style = a.parse().expect("checked");
                Ok(())
            },
        }),
        summarized: true,
        prompt: true,
        flag: true,
    },
    Verb {
        name: "clipboard_cmd",
        cycle: Cycle::No,
        field: None,
        summarized: false,
        prompt: false,
        flag: false,
    },
    Verb {
        name: "map",
        cycle: Cycle::No,
        field: None,
        summarized: false,
        prompt: true,
        flag: false,
    },
    Verb {
        name: "clear_all_shortcuts",
        cycle: Cycle::No,
        field: None,
        summarized: false,
        prompt: false,
        flag: false,
    },
    Verb {
        name: "open_cmd",
        cycle: Cycle::Toggle,
        field: None,
        summarized: false,
        prompt: false,
        flag: false,
    },
    Verb {
        name: "one_page_rows_below",
        cycle: Cycle::No,
        field: Some(Field {
            get: |s| s.one_page_rows_below.to_string(),
            set: |s, a| {
                s.one_page_rows_below = positive(a)?;
                Ok(())
            },
        }),
        summarized: false,
        prompt: false,
        flag: true,
    },
];

/// What cycle takes, in order.
pub fn names() -> impl Iterator<Item = &'static str> {
    VERBS
        .iter()
        .filter(|v| v.cycle != Cycle::No)
        .map(|v| v.name)
}

pub fn verb(name: &str) -> Option<&'static Verb> {
    VERBS.iter().find(|v| v.name == name)
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

/// A verb with values.
pub fn lookup(name: &str) -> Option<&'static Verb> {
    verb(name).filter(|v| !v.values().is_empty())
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
    /// What the verb takes, when cycle steps through them.
    pub fn values(&self) -> &'static [&'static str] {
        match self.cycle {
            Cycle::Values(v) => v,
            _ => &[],
        }
    }

    pub fn yes_no(&self) -> bool {
        self.cycle == YES_NO
    }

    /// The setting's value as a config file spells it; "" for a verb
    /// that is no setting.
    pub fn value(&self, s: &Settings) -> String {
        self.field.as_ref().map_or_else(String::new, |f| (f.get)(s))
    }

    /// The value after `v`: the first after the last, or after a value
    /// not in the list.
    pub fn next(&self, v: &str) -> &'static str {
        let values = self.values();
        match values.iter().position(|&x| x == v) {
            Some(i) => values[(i + 1) % values.len()],
            None => values[0],
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
        assert_eq!(s.values(), ["char", "word"]);
        for (cur, next) in [("char", "word"), ("word", "char"), ("", "char")] {
            assert_eq!(s.next(cur), next, "{cur:?}");
        }
        assert!(lookup("tab_width").is_none());
    }

    /// Each setting's value, as spelled, is what its verb takes, and
    /// so is each value it lists; a number must be positive.
    #[test]
    fn fields_round_trip() {
        let fields: Vec<&str> = VERBS
            .iter()
            .filter(|v| v.field.is_some())
            .map(|v| v.name)
            .collect();
        assert_eq!(
            fields,
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
        for v in VERBS {
            let Some(f) = &v.field else { continue };
            let values: &[&str] = if v.values().is_empty() {
                &["7"]
            } else {
                v.values()
            };
            for &want in values {
                let mut s = Settings::default();
                (f.set)(&mut s, want).unwrap();
                assert_eq!(v.value(&s), want, "{}", v.name);
            }
        }
        let mut s = Settings::default();
        for bad in ["0", "65536", "x", ""] {
            let err = (verb("tab_width").unwrap().field.as_ref().unwrap().set)(&mut s, bad);
            assert_eq!(err, Err("a number from 1 to 65535"), "{bad:?}");
        }
        assert_eq!(verb("map").unwrap().value(&s), "");
    }

    /// The prompt takes every verb but the three a running pager
    /// cannot use.
    #[test]
    fn prompt_verbs() {
        let out: Vec<&str> = VERBS.iter().filter(|v| !v.prompt).map(|v| v.name).collect();
        assert_eq!(
            out,
            [
                "clipboard_cmd",
                "clear_all_shortcuts",
                "open_cmd",
                "one_page_rows_below"
            ]
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
