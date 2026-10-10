/// A config setting cycle steps through, and its values in order.
pub struct Setting {
    pub name: &'static str,
    pub values: &'static [&'static str],
}

const YES_NO: &[&str] = &["yes", "no"];

pub const SETTINGS: &[Setting] = &[
    Setting {
        name: "wrap",
        values: YES_NO,
    },
    Setting {
        name: "wrap_style",
        values: &["char", "word"],
    },
    Setting {
        name: "edge_markers",
        values: YES_NO,
    },
    Setting {
        name: "file_separators",
        values: YES_NO,
    },
    Setting {
        name: "auto_reload",
        values: YES_NO,
    },
    Setting {
        name: "status_line",
        values: YES_NO,
    },
    Setting {
        name: "view_style",
        values: &["color", "plain", "raw"],
    },
];

/// What cycle takes, in order.
pub fn names() -> impl Iterator<Item = &'static str> {
    SETTINGS.iter().map(|s| s.name)
}

pub fn lookup(name: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.name == name)
}

impl Setting {
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
}
