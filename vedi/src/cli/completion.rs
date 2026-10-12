//! The shell completion scripts.

/// The completion script for `shell`; `None` for one vedi has none for.
pub fn completion(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(include_str!("../../completions/vedi.bash")),
        "zsh" => Some(include_str!("../../completions/_vedi")),
        "fish" => Some(include_str!("../../completions/vedi.fish")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::*;
    use crate::cli::USAGE;

    const SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

    /// Every flag the usage text names, a --[no-] pair as both.
    fn usage_flags() -> Vec<String> {
        let mut flags = Vec::new();
        for line in USAGE.lines().filter(|l| l.starts_with("  -")) {
            let line = line.trim();
            let spec = line.split_once("  ").map_or(line, |(s, _)| s);
            for f in spec.split(", ") {
                let f = f.split_once(' ').map_or(f, |(f, _)| f);
                match f.strip_prefix("--[no-]") {
                    Some(rest) => flags.extend([format!("--{rest}"), format!("--no-{rest}")]),
                    None => flags.push(f.to_string()),
                }
            }
        }
        flags
    }

    #[test]
    fn usage_flags_found() {
        let got = format!(" {} ", usage_flags().join(" "));
        for f in [
            "-S",
            "--wrap",
            "--no-wrap",
            "--wrap-style",
            "-F",
            "--no-quit-if-one-page",
            "--completion",
            "-h",
            "--version",
        ] {
            assert!(got.contains(&format!(" {f} ")), "{f} missing from {got}");
        }
    }

    fn word_char(c: char) -> bool {
        c.is_ascii_lowercase() || c == '-' || c == '_'
    }

    /// `f` stands as a word of its own: no letter or - either side.
    fn names(script: &str, f: &str) -> bool {
        script.match_indices(f).any(|(i, _)| {
            !script[..i].chars().next_back().is_some_and(word_char)
                && !script[i + f.len()..].chars().next().is_some_and(word_char)
        })
    }

    /// Fish names a flag `-l name` or `-s c`, then a space or the line's end.
    fn fish_names(script: &str, f: &str) -> bool {
        let opt = match f.strip_prefix("--") {
            Some(name) => format!("-l {name}"),
            None => format!("-s {}", &f[1..]),
        };
        script.match_indices(&opt).any(|(i, _)| {
            matches!(
                script[i + opt.len()..].chars().next(),
                None | Some(' ' | '\n')
            )
        })
    }

    /// A flag added to the usage text fails here until each script
    /// completes it.
    #[test]
    fn completion_flags() {
        for sh in SHELLS {
            let script = completion(sh).unwrap_or_else(|| panic!("no {sh} script"));
            for f in usage_flags() {
                let found = if sh == "fish" {
                    fish_names(script, &f)
                } else {
                    names(script, &f)
                };
                assert!(found, "{sh}: no {f}");
            }
        }
    }

    /// The man page with its \- escapes as dashes and its \& escapes
    /// gone. A dash in a flag must be \- so groff draws a minus sign,
    /// not a hyphen; mandoc does not check that.
    fn man_page() -> String {
        let page = include_str!("../../man/vedi.1");
        for line in page.lines() {
            let words: Vec<&str> = line.split(' ').collect();
            for w in words.windows(2).filter(|w| w[0] == "Fl") {
                assert!(
                    !w[1].replace("\\-", "").contains('-'),
                    "man: {} has a bare dash",
                    w[1]
                );
            }
        }
        page.replace("\\-", "-").replace("\\&", "")
    }

    /// A flag added to the usage text fails here until the man page
    /// names it. Fl supplies the first dash.
    #[test]
    fn man_flags() {
        let page = man_page();
        for f in usage_flags() {
            assert!(names(&page, &format!("Fl {}", &f[1..])), "man: no {f}");
        }
    }

    /// Every default binding is in the page as the key it is spelled
    /// with in the config file.
    #[test]
    fn man_keys() {
        let page = man_page();
        let keys = crate::input::keymap::DEFAULTS
            .lines()
            .filter_map(|l| l.strip_prefix("map "))
            .map(|l| l.split(' ').next().unwrap());
        for key in keys {
            assert!(names(&page, &format!("Ic {key}")), "man: no {key}");
        }
    }

    /// Every config verb and every : command is in the page.
    #[test]
    fn man_settings_and_commands() {
        let page = man_page();
        let verbs = crate::input::setting::VERBS.iter().map(|v| v.name);
        for name in verbs.chain(crate::app::command::commands()) {
            assert!(names(&page, &format!("Cm {name}")), "man: no {name}");
        }
    }

    /// mandoc lints the man page where it is installed: macOS ships
    /// it, and the linux CI container installs it. -W warning leaves
    /// out the notes about the host's own man database.
    #[test]
    fn man_syntax() {
        let Some(bin) = on_path("mandoc") else {
            eprintln!("skip: no mandoc");
            return;
        };
        let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("man/vedi.1");
        let out = Command::new(bin)
            .args(["-T", "lint", "-W", "warning"])
            .arg(&page)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "mandoc: {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn completion_unknown_shell() {
        assert_eq!(completion("tcsh"), None);
    }

    /// A directory removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("vedi-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn on_path(bin: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|d| d.join(bin))
            .find(|p| p.is_file())
    }

    /// Runs the script in bash, and in macOS's 3.2 where there is one.
    #[test]
    fn completion_bash() {
        let mut bins: Vec<PathBuf> = on_path("bash").into_iter().collect();
        if Path::new("/bin/bash").exists() && !bins.iter().any(|b| b == Path::new("/bin/bash")) {
            bins.push("/bin/bash".into());
        }
        if bins.is_empty() {
            eprintln!("skip: no bash");
            return;
        }
        let dir = TempDir::new("completion-bash");
        let script = dir.0.join("vedi.bash");
        fs::write(&script, completion("bash").unwrap()).unwrap();
        let work = dir.0.join("work");
        fs::create_dir(&work).unwrap();
        for name in ["a log.txt", "-x.txt", "report[1].txt", "report1.txt"] {
            fs::write(work.join(name), "").unwrap();
        }
        // words is the command line as bash splits it: = is a word of its own.
        let cases: [(&str, &[&str], &str); 16] = [
            ("flag prefix", &["--wr"], "--wrap\n--wrap-style"),
            ("no- form", &["--no-w"], "--no-wrap"),
            ("short flags", &["-F"], "-F"),
            ("wrap style", &["--wrap-style", ""], "char\nword"),
            ("wrap style prefix", &["--wrap-style", "w"], "word"),
            ("wrap style eq", &["--wrap-style", "="], "char\nword"),
            ("wrap style eq prefix", &["--wrap-style", "=", "c"], "char"),
            ("view style", &["--view-style", ""], "color\nplain\nraw"),
            ("shells", &["--completion", ""], "bash\nzsh\nfish"),
            ("number", &["--tab-width", ""], ""),
            ("number eq", &["--tab-width", "="], ""),
            ("config", &["--config", "a"], "a log.txt"),
            ("command", &["--open-cmd", "prin"], "printf"),
            ("file", &["-S", "a"], "a log.txt"),
            ("file after dash dash", &["--", "-"], "-x.txt"),
            ("file with a glob character", &["report["], "report[1].txt"),
        ];
        for (name, words, want) in cases {
            for bash in &bins {
                let out = Command::new(bash)
                    .args(["--norc", "--noprofile", "-c"])
                    .arg(
                        r#"
source "$1"; shift
COMP_WORDS=(vedi "$@"); COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
_vedi
[ ${#COMPREPLY[@]} -eq 0 ] || printf '%s\n' "${COMPREPLY[@]}""#,
                    )
                    .arg("_")
                    .arg(&script)
                    .args(words)
                    .current_dir(&work)
                    .output()
                    .unwrap();
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                assert!(
                    out.status.success(),
                    "{name} {}: {}: {text}",
                    bash.display(),
                    out.status
                );
                let got = text.strip_suffix('\n').unwrap_or(&text);
                // Commands vary by machine; printf is always among them.
                if name == "command" {
                    assert!(
                        format!("\n{got}\n").contains(&format!("\n{want}\n")),
                        "{name} {}: got {got:?}, want {want:?} among them",
                        bash.display()
                    );
                } else {
                    assert_eq!(got, want, "{name} {}", bash.display());
                }
            }
        }
    }

    /// zsh and fish parse their scripts.
    #[test]
    fn completion_syntax() {
        for sh in ["zsh", "fish"] {
            let Some(bin) = on_path(sh) else {
                eprintln!("skip: no {sh}");
                continue;
            };
            let dir = TempDir::new(&format!("completion-{sh}"));
            let path = dir.0.join(format!("vedi.{sh}"));
            fs::write(&path, completion(sh).unwrap()).unwrap();
            let out = Command::new(bin).arg("-n").arg(&path).output().unwrap();
            assert!(
                out.status.success(),
                "{sh}: {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
