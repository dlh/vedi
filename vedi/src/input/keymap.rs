use crate::input::action::Command;
use crate::input::key::Key;

/// The default bindings, read by the config parser.
pub const DEFAULTS: &str = include_str!("defaults.conf");

/// A key and what it does; `None` unbinds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Binding {
    pub key: Key,
    pub cmd: Option<Command>,
}

/// The bindings in effect, one per key, in order: the help screen
/// lists keys in this order.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Keymap(Vec<Binding>);

impl Keymap {
    /// `bs` applied, in order, to an empty map.
    pub fn new(bs: Vec<Binding>) -> Keymap {
        Keymap::default().apply(&bs)
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.0
    }

    /// A copy of the map with `bs` laid over it: a bound key is
    /// replaced in place, a new one appended, and `None` removes it.
    pub fn apply(&self, bs: &[Binding]) -> Keymap {
        let mut m = self.0.clone();
        for b in bs {
            let i = m.iter().position(|x| x.key == b.key);
            match (i, &b.cmd) {
                (Some(i), None) => {
                    m.remove(i);
                }
                (None, None) => {}
                (Some(i), Some(_)) => m[i] = b.clone(),
                (None, Some(_)) => m.push(b.clone()),
            }
        }
        Keymap(m)
    }

    /// What `key` does, normalized as a parsed name is.
    pub fn lookup(&self, key: Key) -> Option<Command> {
        let key = key.normalized();
        self.0.iter().find(|b| b.key == key)?.cmd.clone()
    }

    /// The first key bound to `cmd`.
    pub fn find(&self, cmd: &Command) -> Option<Key> {
        self.0
            .iter()
            .find(|b| b.cmd.as_ref() == Some(cmd))
            .map(|b| b.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::action::Action;
    use crate::input::key::{KeyCode, Mods};

    fn key(name: &str) -> Key {
        name.parse().unwrap()
    }

    fn bind(k: &str, action: Option<Action>) -> Binding {
        Binding {
            key: key(k),
            cmd: action.map(|action| Command {
                action,
                extend: false,
                arg: None,
            }),
        }
    }

    #[test]
    fn apply() {
        let m = Keymap::new(vec![
            bind("h", Some(Action::Help)),
            bind("q", Some(Action::Quit)),
        ]);
        let got = m.apply(&[
            bind("h", Some(Action::Left)),
            bind("x", Some(Action::Help)),
            bind("q", None),
            bind("z", None),
        ]);
        let want = [bind("h", Some(Action::Left)), bind("x", Some(Action::Help))];
        assert_eq!(got.bindings(), want);
        assert_eq!(
            m.bindings()[0],
            bind("h", Some(Action::Help)),
            "receiver changed"
        );
    }

    /// A key bound twice does what its later binding says, in the place
    /// of its first.
    #[test]
    fn later_binding_wins() {
        let m = Keymap::new(vec![
            bind("j", Some(Action::Down)),
            bind("k", Some(Action::Up)),
            bind("j", Some(Action::Quit)),
        ]);
        assert_eq!(
            m.bindings(),
            [bind("j", Some(Action::Quit)), bind("k", Some(Action::Up))]
        );
        assert_eq!(m.lookup(key("j")), bind("j", Some(Action::Quit)).cmd);
    }

    #[test]
    fn lookup_and_find() {
        let m = Keymap::new(vec![
            bind("Ctrl+f", Some(Action::PageDown)),
            bind("f", Some(Action::PageDown)),
        ]);
        let page_down = bind("f", Some(Action::PageDown)).cmd;
        assert_eq!(
            m.lookup(Key::new(KeyCode::Char('\x06'), Mods::NONE)),
            page_down
        );
        assert_eq!(m.lookup(key("g")), None);
        assert_eq!(m.find(page_down.as_ref().unwrap()), Some(key("Ctrl+f")));
        assert_eq!(m.find(&bind("q", Some(Action::Quit)).cmd.unwrap()), None);
    }
}
