/// A section of a scenario file: "-- name --" then its body, ending in
/// a newline unless empty. `line` is the header's 1-based line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Section {
    pub name: String,
    pub body: String,
    pub line: usize,
}

impl Section {
    /// The name without its argument: "reload" for "reload b.txt".
    pub fn verb(&self) -> &str {
        self.name.split_once(' ').map_or(&self.name, |(v, _)| v)
    }

    pub fn arg(&self) -> &str {
        self.name.split_once(' ').map_or("", |(_, a)| a)
    }
}

pub struct Archive {
    pub comment: String,
    pub sections: Vec<Section>,
}

/// Splits txtar-style text: everything before the first "-- name --"
/// line is the comment.
pub fn parse(data: &str) -> Archive {
    let mut data = data.to_string();
    if !data.is_empty() && !data.ends_with('\n') {
        data.push('\n');
    }
    let mut a = Archive {
        comment: String::new(),
        sections: Vec::new(),
    };
    let mut cur: Option<Section> = None;
    for (i, line) in data.split_inclusive('\n').enumerate() {
        let trimmed = line.strip_suffix('\n').unwrap_or(line);
        if let Some(name) = trimmed
            .strip_prefix("-- ")
            .and_then(|s| s.strip_suffix(" --"))
        {
            if let Some(sec) = cur.take() {
                a.sections.push(sec);
            }
            cur = Some(Section {
                name: name.to_string(),
                body: String::new(),
                line: i + 1,
            });
            continue;
        }
        match &mut cur {
            Some(sec) => sec.body.push_str(line),
            None => a.comment.push_str(line),
        }
    }
    if let Some(sec) = cur {
        a.sections.push(sec);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_archive() {
        let a = parse(
            "prose\nmore\n-- size --\n10x4\n-- input --\nline 1\n\n-- empty --\n-- last --\nx\n",
        );
        assert_eq!(a.comment, "prose\nmore\n");
        let sec = |name: &str, body: &str, line| Section {
            name: name.into(),
            body: body.into(),
            line,
        };
        assert_eq!(
            a.sections,
            [
                sec("size", "10x4\n", 3),
                sec("input", "line 1\n\n", 5),
                sec("empty", "", 8),
                sec("last", "x\n", 9),
            ]
        );
        let a = parse("prose\n-- last --\nx");
        assert_eq!(
            a.sections.last().unwrap().body,
            "x\n",
            "without a final newline"
        );
        let s = sec("reload b.txt", "", 1);
        assert_eq!((s.verb(), s.arg()), ("reload", "b.txt"));
        assert_eq!((a.sections[0].verb(), a.sections[0].arg()), ("last", ""));
    }
}
