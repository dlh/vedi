/// Splits `s` into words as a shell would: on blanks, with '...' taken
/// as written, "..." too but for \" and \\, and a \ outside quotes
/// standing for the character after it.
pub fn split(s: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut w = String::new();
    let mut in_word = false;
    let mut quote = None;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            _ if quote == Some(c) => quote = None,
            _ if quote == Some('\'') => w.push(c),
            '\\' => {
                let Some(&next) = it.peek() else {
                    return Err("trailing \\".to_string());
                };
                if quote == Some('"') && next != '"' && next != '\\' {
                    w.push(c);
                    continue;
                }
                it.next();
                w.push(next);
                in_word = true;
            }
            _ if quote == Some('"') => w.push(c),
            '\'' | '"' => {
                quote = Some(c);
                in_word = true;
            }
            ' ' | '\t' | '\n' => {
                if in_word {
                    words.push(std::mem::take(&mut w));
                    in_word = false;
                }
            }
            _ => {
                w.push(c);
                in_word = true;
            }
        }
    }
    if let Some(q) = quote {
        return Err(format!("unclosed {q}"));
    }
    if in_word {
        words.push(w);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_words() {
        let tests: &[(&str, &[&str])] = &[
            ("", &[]),
            (" \t\n ", &[]),
            ("-F -S", &["-F", "-S"]),
            ("bat --color=always %s", &["bat", "--color=always", "%s"]),
            ("a 'b c' d", &["a", "b c", "d"]),
            (r#"a "b c" d"#, &["a", "b c", "d"]),
            (r#"--x="y z""#, &["--x=y z"]),
            (r"my\ file", &["my file"]),
            (r#""a \"b\" \\ \c""#, &[r#"a "b" \ \c"#]),
            (r"'a\b'", &[r"a\b"]),
            ("''", &[""]),
            ("sed -e 's/a/b/' %s", &["sed", "-e", "s/a/b/", "%s"]),
        ];
        for &(input, want) in tests {
            let want: Vec<String> = want.iter().map(|w| w.to_string()).collect();
            assert_eq!(split(input), Ok(want), "split({input:?})");
        }
        for (input, err) in [
            ("'a", "unclosed '"),
            (r#""a"#, r#"unclosed ""#),
            (r"a \", r"trailing \"),
        ] {
            assert_eq!(split(input), Err(err.to_string()), "split({input:?})");
        }
    }
}
