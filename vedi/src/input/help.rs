use crate::input::action::{Action, Command};
use crate::input::key::Mods;
use crate::input::keymap::Keymap;

/// One line of the help screen: the keys bound to its actions, in map
/// order, and what they do. Fixed rows, for the mouse and the prompt,
/// name their keys themselves.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub keys: Vec<String>,
    pub doc: String,
}

/// A heading and the rows under it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Section {
    pub name: String,
    pub rows: Vec<Row>,
}

/// A [`Command`] the table can hold in a const.
#[derive(Copy, Clone)]
enum HelpCmd {
    Act(Action),
    Sel(Action),
    Cyc(&'static str),
}

impl PartialEq<Command> for HelpCmd {
    fn eq(&self, c: &Command) -> bool {
        match *self {
            HelpCmd::Act(a) => c.action == a && !c.extend && c.arg.is_none(),
            HelpCmd::Sel(a) => c.action == a && c.extend && c.arg.is_none(),
            HelpCmd::Cyc(s) => {
                c.action == Action::Cycle && !c.extend && c.arg.as_deref() == Some(s)
            }
        }
    }
}

enum Keys {
    Bound(&'static [HelpCmd]),
    /// Keys named outright, for what no action binds.
    Fixed(&'static [&'static str]),
}

struct HelpRow {
    keys: Keys,
    doc: &'static str,
}

struct HelpSection {
    name: &'static str,
    rows: &'static [HelpRow],
}

const fn bound(cmds: &'static [HelpCmd], doc: &'static str) -> HelpRow {
    HelpRow {
        keys: Keys::Bound(cmds),
        doc,
    }
}

const fn fixed(keys: &'static [&'static str], doc: &'static str) -> HelpRow {
    HelpRow {
        keys: Keys::Fixed(keys),
        doc,
    }
}

/// The help screen in order: the first, unnamed, is shown at the top
/// as less shows h and q. The table in docs/keys.md is the same rows
/// with the macOS keys.
const SECTIONS: &[HelpSection] = {
    use Action::*;
    use HelpCmd::*;
    &[
        HelpSection {
            name: "",
            rows: &[
                bound(&[Act(Help)], "Show the key bindings"),
                bound(&[Act(Quit)], "Quit"),
                bound(&[Cyc("wrap")], "Toggle wrap / nowrap"),
                bound(&[Cyc("wrap_style")], "Cycle the wrap style: char, word"),
                bound(&[Cyc("edge_markers")], "Toggle the edge markers"),
                bound(
                    &[Cyc("file_separators")],
                    "Toggle the rows naming the inputs",
                ),
                bound(&[Cyc("auto_reload")], "Toggle reloading a changed file"),
                bound(&[Cyc("status_line")], "Toggle the status line"),
                bound(
                    &[Cyc("view_style")],
                    "Cycle the view style: color, plain, raw",
                ),
                bound(&[Act(Reload)], "Reload the file"),
                bound(&[Cyc("open_cmd")], "Toggle reading through open_cmd"),
            ],
        },
        HelpSection {
            name: "Moving",
            rows: &[
                bound(
                    &[
                        Act(Up),
                        Act(Down),
                        Act(Left),
                        Act(Right),
                        Act(Home),
                        Act(End),
                    ],
                    "Move the cursor",
                ),
                bound(&[Act(PageDown)], "Down a page"),
                bound(&[Act(PageUp)], "Up a page"),
                bound(&[Act(HalfPageDown)], "Down half a page"),
                bound(&[Act(HalfPageUp)], "Up half a page"),
                bound(&[Act(WordLeft), Act(WordRight)], "Move by word"),
                bound(&[Act(First), Act(Last)], "Start, end of the text"),
                bound(
                    &[Act(ScreenTop), Act(ScreenMiddle), Act(ScreenBottom)],
                    "Top, middle, bottom row of the screen",
                ),
                bound(
                    &[Act(PrevFile), Act(NextFile)],
                    "Previous, next input or title",
                ),
                bound(&[Act(CommandPrompt)], "Command prompt; N goes to line N"),
            ],
        },
        HelpSection {
            name: "Selecting",
            rows: &[
                bound(
                    &[
                        Sel(Up),
                        Sel(Down),
                        Sel(Left),
                        Sel(Right),
                        Sel(Home),
                        Sel(End),
                        Sel(First),
                        Sel(Last),
                        Sel(PrevFile),
                        Sel(NextFile),
                    ],
                    "Extend the selection",
                ),
                bound(&[Sel(PageDown), Sel(PageUp)], "Extend by a page"),
                bound(
                    &[Sel(HalfPageDown), Sel(HalfPageUp)],
                    "Extend by half a page",
                ),
                bound(
                    &[Sel(ScreenTop), Sel(ScreenMiddle), Sel(ScreenBottom)],
                    "Extend to the top, middle, bottom row",
                ),
                bound(&[Sel(WordLeft), Sel(WordRight)], "Extend by word"),
                bound(
                    &[Act(SetMark)],
                    "Start a selection that motions extend; again, end it",
                ),
                bound(&[Act(SelectAll)], "Select all"),
                bound(
                    &[Act(ClearSelection)],
                    "Clear the selection, then the search highlight",
                ),
            ],
        },
        HelpSection {
            name: "Mouse",
            rows: &[
                fixed(
                    &["Click, drag"],
                    "Move the cursor, select; with Shift, extend; held at an edge, scroll",
                ),
                fixed(&["Double-, triple-click"], "Select the word, the line"),
                fixed(&["Wheel"], "Scroll; with Shift, sideways"),
            ],
        },
        HelpSection {
            name: "Copying",
            rows: &[
                bound(&[Act(Copy)], "Copy the selection as plain text"),
                bound(
                    &[Act(CopyAndQuit)],
                    "Copy the selection and quit; with none, down a line",
                ),
            ],
        },
        HelpSection {
            name: "Searching",
            rows: &[
                bound(
                    &[Act(Search)],
                    "Search; ignores case if lowercase; empty repeats",
                ),
                bound(&[Act(SearchBack)], "Search backward"),
                bound(
                    &[Act(SearchNext), Act(SearchPrev)],
                    "Next and previous match; ? swaps them",
                ),
                bound(
                    &[Act(Filter)],
                    "Filter: list the lines that match; Enter goes to one",
                ),
            ],
        },
        HelpSection {
            name: "Prompts",
            rows: &[
                fixed(&["Up", "Down"], "Recall earlier lines at this prompt"),
                fixed(
                    &["Ctrl+n", "Ctrl+p", "PgDn", "PgUp"],
                    "Move the pointer at the & prompt",
                ),
                fixed(
                    &["Left", "Right", "Home", "End", "Cmd+Left", "Cmd+Right"],
                    "Move in the line",
                ),
                fixed(
                    &[
                        "Ctrl+Left",
                        "Ctrl+Right",
                        "Alt+Left",
                        "Alt+Right",
                        "Alt+b",
                        "Alt+f",
                    ],
                    "Move by word",
                ),
                fixed(
                    &["Alt+Backspace", "Ctrl+Backspace", "Ctrl+w"],
                    "Delete the word before the cursor",
                ),
                fixed(
                    &["Cmd+Backspace", "Ctrl+u"],
                    "Delete to the start of the line",
                ),
                fixed(&["Tab"], "Complete; again, the next match"),
                fixed(&["Esc", "Ctrl+g", "Ctrl+c"], "Cancel"),
            ],
        },
    ]
};

impl Keymap {
    /// The help screen for the map: a row none of whose actions has a
    /// key is left out, and a section with no rows left. A fixed Cmd+
    /// key is listed where the map binds any, that is on macOS.
    pub fn help(&self) -> Vec<Section> {
        let cmd = self
            .bindings()
            .iter()
            .any(|b| b.key.mods.contains(Mods::CMD));
        let mut secs = Vec::new();
        for s in SECTIONS {
            let mut rows = Vec::new();
            for row in s.rows {
                let keys: Vec<String> = match row.keys {
                    Keys::Fixed(keys) => keys
                        .iter()
                        .filter(|k| cmd || !k.starts_with("Cmd+"))
                        .map(|k| k.to_string())
                        .collect(),
                    Keys::Bound(cmds) => self
                        .bindings()
                        .iter()
                        .filter(|b| b.cmd.as_ref().is_some_and(|c| cmds.iter().any(|h| h == c)))
                        .map(|b| b.key.to_string())
                        .collect(),
                };
                if matches!(row.keys, Keys::Fixed(_)) || !keys.is_empty() {
                    rows.push(Row {
                        keys,
                        doc: row.doc.to_string(),
                    });
                }
            }
            if !rows.is_empty() {
                secs.push(Section {
                    name: s.name.to_string(),
                    rows,
                });
            }
        }
        secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::action::Action;
    use crate::input::key::Key;
    use crate::input::keymap::Binding;
    use crate::input::setting;

    /// Every action a user can map has a row, or its binding would
    /// vanish from the help screen.
    #[test]
    fn help_covers_actions() {
        let covered: Vec<HelpCmd> = SECTIONS
            .iter()
            .flat_map(|s| s.rows)
            .flat_map(|r| match r.keys {
                Keys::Bound(cmds) => cmds,
                Keys::Fixed(_) => &[],
            })
            .copied()
            .collect();
        let has = |c: Command| covered.iter().any(|h| *h == c);
        for name in setting::names() {
            assert!(
                has(Command {
                    action: Action::Cycle,
                    extend: false,
                    arg: Some(name.to_string())
                }),
                "cycle {name} has no help row"
            );
        }
        for a in Action::ALL {
            if a == Action::Cycle {
                continue;
            }
            let c = Command {
                action: a,
                extend: false,
                arg: None,
            };
            assert!(has(c.clone()), "{a} has no help row");
            if a.is_movement() {
                assert!(
                    has(Command { extend: true, ..c }),
                    "select_{a} has no help row"
                );
            }
        }
    }

    /// Keys come in map order; a row with no key bound is left out, and
    /// a section with no rows; fixed rows stay, less their Cmd+ keys
    /// where no binding has Cmd.
    #[test]
    fn help_lists_bound_keys() {
        let bind = |k: &str, c: &str| Binding {
            key: k.parse::<Key>().unwrap(),
            cmd: Some(c.parse().unwrap()),
        };
        let m = Keymap::new(vec![
            bind("q", "quit"),
            bind("Right", "right"),
            bind("Up", "up"),
            bind("w", "cycle wrap"),
        ]);
        let secs = m.help();
        let names: Vec<&str> = secs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["", "Moving", "Mouse", "Prompts"]);
        let row = |keys: &[&str], doc: &str| Row {
            keys: keys.iter().map(|k| k.to_string()).collect(),
            doc: doc.to_string(),
        };
        assert_eq!(
            secs[0].rows,
            [row(&["q"], "Quit"), row(&["w"], "Toggle wrap / nowrap")]
        );
        assert_eq!(secs[1].rows, [row(&["Right", "Up"], "Move the cursor")]);
        assert_eq!(
            secs[3].rows[2],
            row(&["Left", "Right", "Home", "End"], "Move in the line")
        );
        assert_eq!(
            secs[3].rows[5],
            row(&["Ctrl+u"], "Delete to the start of the line")
        );
        assert_eq!(secs[3].rows[7], row(&["Esc", "Ctrl+g", "Ctrl+c"], "Cancel"));
    }
}
