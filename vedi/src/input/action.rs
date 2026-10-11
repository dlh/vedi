use std::fmt;
use std::str::FromStr;

use crate::input::setting;

/// Movement actions come first, so `is_movement` is a range.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    WordLeft,
    WordRight,
    First,
    Last,
    PrevFile,
    NextFile,
    SelectAll,
    ClearSelection,
    SetMark,
    Copy,
    CopyAndQuit,
    Search,
    SearchBack,
    SearchNext,
    SearchPrev,
    CommandPrompt,
    Cycle,
    Quit,
    Help,
    Reload,
}

/// In declaration order: `name` indexes it.
const NAMES: [(Action, &str); 30] = [
    (Action::Up, "up"),
    (Action::Down, "down"),
    (Action::Left, "left"),
    (Action::Right, "right"),
    (Action::Home, "home"),
    (Action::End, "end"),
    (Action::PageUp, "page_up"),
    (Action::PageDown, "page_down"),
    (Action::HalfPageUp, "half_page_up"),
    (Action::HalfPageDown, "half_page_down"),
    (Action::WordLeft, "word_left"),
    (Action::WordRight, "word_right"),
    (Action::First, "first"),
    (Action::Last, "last"),
    (Action::PrevFile, "prev_file"),
    (Action::NextFile, "next_file"),
    (Action::SelectAll, "select_all"),
    (Action::ClearSelection, "clear_selection"),
    (Action::SetMark, "set_mark"),
    (Action::Copy, "copy"),
    (Action::CopyAndQuit, "copy_and_quit"),
    (Action::Search, "search"),
    (Action::SearchBack, "search_back"),
    (Action::SearchNext, "search_next"),
    (Action::SearchPrev, "search_prev"),
    (Action::CommandPrompt, "command"),
    (Action::Cycle, "cycle"),
    (Action::Quit, "quit"),
    (Action::Help, "help"),
    (Action::Reload, "reload"),
];

/// The name that unbinds a key.
const NONE: &str = "none";

impl Action {
    pub const ALL: [Action; 30] = {
        let mut all = [Action::Up; 30];
        let mut i = 0;
        while i < 30 {
            all[i] = NAMES[i].0;
            i += 1;
        }
        all
    };

    pub fn is_movement(self) -> bool {
        (Action::Up as u8..=Action::NextFile as u8).contains(&(self as u8))
    }

    fn name(self) -> &'static str {
        NAMES[self as usize].1
    }
}

/// The config name.
impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Action {
    type Err = ActionError;

    fn from_str(name: &str) -> Result<Action, ActionError> {
        NAMES
            .iter()
            .find(|&&(_, n)| n == name)
            .map(|&(a, _)| a)
            .ok_or_else(|| ActionError::Unknown(name.to_string()))
    }
}

/// A decoded key. `extend` is Shift on a movement: extend the
/// selection instead of clearing it. `arg` is Cycle's setting.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Command {
    pub action: Action,
    pub extend: bool,
    pub arg: Option<String>,
}

impl Command {
    /// Every name `parse_binding` takes but the alias, sorted.
    pub fn names() -> Vec<String> {
        let mut names = vec![NONE.to_string()];
        for (a, n) in NAMES {
            names.push(n.to_string());
            if a.is_movement() {
                names.push(format!("select_{n}"));
            }
        }
        names.sort();
        names
    }

    /// What a key is bound to: a command, or `None` for none, which
    /// unbinds it.
    pub fn parse_binding(text: &str) -> Result<Option<Command>, ActionError> {
        match text.split_whitespace().collect::<Vec<_>>()[..] {
            [NONE] => Ok(None),
            _ => text.parse().map(Some),
        }
    }
}

/// The config name of what a key is bound to; `parse_binding` reads it
/// back.
pub fn binding_name(cmd: &Option<Command>) -> String {
    cmd.as_ref()
        .map_or_else(|| NONE.to_string(), Command::to_string)
}

/// The config name: select_ before a movement that extends, cycle and
/// its setting.
impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.extend {
            write!(f, "select_")?;
        }
        write!(f, "{}", self.action)?;
        match &self.arg {
            Some(arg) => write!(f, " {arg}"),
            None => Ok(()),
        }
    }
}

/// An action, or cycle and its setting. go_to_line is command's old
/// name.
impl FromStr for Command {
    type Err = ActionError;

    fn from_str(text: &str) -> Result<Command, ActionError> {
        let f: Vec<&str> = text.split_whitespace().collect();
        if f.first() == Some(&"cycle") {
            return match f[..] {
                [_, name] if setting::lookup(name).is_some() => Ok(Command {
                    action: Action::Cycle,
                    extend: false,
                    arg: Some(name.to_string()),
                }),
                _ => Err(ActionError::Cycle),
            };
        }
        let mut name = f.join(" ");
        if name == "go_to_line" {
            name = "command".to_string();
        }
        if let Ok(action) = name.parse::<Action>() {
            return Ok(Command {
                action,
                extend: false,
                arg: None,
            });
        }
        match name.strip_prefix("select_").map(str::parse::<Action>) {
            Some(Ok(action)) if action.is_movement() => Ok(Command {
                action,
                extend: true,
                arg: None,
            }),
            _ => Err(ActionError::Unknown(name)),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ActionError {
    Unknown(String),
    /// cycle without one of the settings.
    Cycle,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ActionError::Unknown(name) => write!(f, "unknown action {name:?}"),
            ActionError::Cycle => {
                let names: Vec<&str> = setting::names().collect();
                write!(f, "cycle takes {}", setting::or_list(&names))
            }
        }
    }
}

impl std::error::Error for ActionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::setting;

    fn cmd(action: Action) -> Command {
        Command {
            action,
            extend: false,
            arg: None,
        }
    }

    #[test]
    fn names_in_declaration_order() {
        for (i, a) in Action::ALL.into_iter().enumerate() {
            assert_eq!(a as usize, i, "{a:?}");
        }
    }

    #[test]
    fn action_names() {
        for a in Action::ALL {
            if a == Action::Cycle {
                continue; // takes a setting; cycle()
            }
            let name = a.to_string();
            assert_eq!(name.parse::<Command>(), Ok(cmd(a)), "{name}");
            assert_eq!(name.parse::<Action>(), Ok(a), "{name}");
            if !a.is_movement() {
                continue;
            }
            let sel = format!("select_{name}");
            let want = Command {
                extend: true,
                ..cmd(a)
            };
            assert_eq!(sel.parse::<Command>(), Ok(want.clone()), "{sel}");
            assert_eq!(want.to_string(), sel);
        }
        for name in [
            "select_all_x",
            "select_copy",
            "select_none",
            "Up",
            "page-down",
            "",
            "toggle_wrap",
            "left now",
            "none",
        ] {
            assert!(name.parse::<Command>().is_err(), "{name:?} parsed");
        }
        assert_eq!(cmd(Action::CopyAndQuit).to_string(), "copy_and_quit");
        assert_eq!(cmd(Action::SetMark).to_string(), "set_mark");
        assert_eq!(cmd(Action::SelectAll).to_string(), "select_all");
    }

    /// Every name parses back to itself, the select_ forms and none
    /// included; cycle, which needs a setting, is listed and parses
    /// with each.
    #[test]
    fn command_names() {
        let names = Command::names();
        let has = |n: &str| names.iter().any(|x| x == n);
        assert!(has("none") && has("select_page_down") && has("cycle") && !has("select_copy"));
        assert!(names.is_sorted(), "{names:?}");
        for n in &names {
            if n == "cycle" {
                continue;
            }
            let c = Command::parse_binding(n).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert_eq!(&binding_name(&c), n);
        }
    }

    /// cycle takes one of the settings, in any spacing, and prints back
    /// as cycle and the setting; alone or with anything else it is an
    /// error naming the settings.
    #[test]
    fn cycle() {
        let want = [
            "wrap",
            "wrap_style",
            "edge_markers",
            "file_separators",
            "auto_reload",
            "status_line",
            "view_style",
        ];
        assert_eq!(setting::names().collect::<Vec<_>>(), want);
        for name in want {
            let c: Command = format!("cycle  {name}").parse().unwrap();
            assert_eq!(
                c,
                Command {
                    arg: Some(name.to_string()),
                    ..cmd(Action::Cycle)
                }
            );
            assert_eq!(c.to_string(), format!("cycle {name}"));
        }
        for text in ["cycle", "cycle tab_width", "cycle wrap yes"] {
            let err = text.parse::<Command>().unwrap_err();
            assert_eq!(
                err.to_string(),
                "cycle takes wrap, wrap_style, edge_markers, file_separators, auto_reload, status_line or view_style",
                "{text}"
            );
        }
        let err = "left wrap".parse::<Command>().unwrap_err();
        assert_eq!(err.to_string(), r#"unknown action "left wrap""#);
    }

    /// go_to_line, the command prompt's old name, still parses; command
    /// is the name it prints.
    #[test]
    fn go_to_line_alias() {
        for name in ["command", "go_to_line"] {
            let c: Command = name.parse().unwrap();
            assert_eq!(c, cmd(Action::CommandPrompt), "{name}");
            assert_eq!(c.to_string(), "command");
        }
        assert!(!Command::names().iter().any(|n| n == "go_to_line"));
    }

    #[test]
    fn parse_binding() {
        assert_eq!(Command::parse_binding(" none "), Ok(None));
        assert_eq!(Command::parse_binding("quit"), Ok(Some(cmd(Action::Quit))));
        assert_eq!(
            Command::parse_binding("cycle wrap").map(|c| binding_name(&c)),
            Ok("cycle wrap".to_string())
        );
        assert!(Command::parse_binding("none quit").is_err());
        assert!(Command::parse_binding("select_none").is_err());
    }
}
