use vedi::input::key::{Key, KeyCode, Mods};

/// A key from a "-- keys --" body; `typed` for a character of quoted
/// text, meant for a prompt.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Press {
    pub key: Key,
    pub typed: bool,
    /// The text the terminal says the key typed.
    pub text: Option<String>,
}

/// Turns a "-- keys --" body into keys: named keys with optional
/// Shift+/Ctrl+/Alt+/Cmd+ prefixes, single characters as themselves,
/// "quoted text" typed char by char, KEY:"text" a key with its text.
pub fn parse_keys(s: &str) -> Result<Vec<Press>, String> {
    let mut keys = Vec::new();
    let mut s = s.trim();
    while !s.is_empty() {
        if let Some(rest) = s.strip_prefix('"') {
            let Some(end) = rest.find('"') else {
                return Err(format!("unterminated quote: {s}"));
            };
            keys.extend(rest[..end].chars().map(|c| Press {
                key: Key::new(KeyCode::Char(c), Mods::NONE),
                typed: true,
                text: None,
            }));
            s = rest[end + 1..].trim();
            continue;
        }
        let (tok, rest) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        let with_text = tok.strip_suffix('"').and_then(|t| t.split_once(":\""));
        let (name, text) = match with_text {
            Some((name, text)) if !name.is_empty() => (name, Some(text.to_string())),
            _ => (tok, None),
        };
        let key = name.parse::<Key>().map_err(|e| e.to_string())?;
        keys.push(Press {
            key,
            typed: false,
            text,
        });
        s = rest.trim();
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let keys = parse_keys(
            r#"Down Shift+Right Ctrl+Shift+Left Alt+Shift+Right Alt+b Ctrl+C Cmd+c Cmd+Shift+g y "a b" Esc Backspace Space"#,
        )
        .unwrap();
        let named = |n: &str| n.parse::<Key>().unwrap();
        let ch = |c| Key::new(KeyCode::Char(c), Mods::NONE);
        let want = [
            named("Down"),
            named("Shift+Right"),
            named("Ctrl+Shift+Left"),
            named("Alt+Shift+Right"),
            named("Alt+b"),
            named("Ctrl+c"),
            named("Cmd+c"),
            named("Cmd+Shift+g"),
            ch('y'),
            ch('a'),
            ch(' '),
            ch('b'),
            named("Esc"),
            named("Backspace"),
            ch(' '),
        ];
        assert_eq!(keys.iter().map(|p| p.key).collect::<Vec<_>>(), want);
        let typed: Vec<bool> = keys.iter().map(|p| p.typed).collect();
        assert_eq!(typed[8..12], [false, true, true, true]);
        let with_text = parse_keys(r#"Alt+v:"√" Alt+::":" q"#).unwrap();
        let want = [
            (named("Alt+v"), Some("√")),
            (named("Alt+:"), Some(":")),
            (ch('q'), None),
        ];
        let got: Vec<_> = with_text
            .iter()
            .map(|p| (p.key, p.text.as_deref()))
            .collect();
        assert_eq!(got, want);
        for bad in ["Bogus", "Shift+y", "\"unterminated", "Bogus:\"x\""] {
            assert!(parse_keys(bad).is_err(), "{bad:?} should fail");
        }
    }
}
