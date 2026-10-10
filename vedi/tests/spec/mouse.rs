/// An action from a "-- mouse --" body.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Act {
    Click,
    DblClick,
    TripleClick,
    Press,
    Release,
    Move,
    Drag,
    Wheel,
    Tick,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// For click, dblclick, tripleclick, press, release and move only the
/// first cell is used; drag presses at the first and releases at the
/// second; all of those take shift, as wheel does with `dir` and `n`;
/// tick uses `n`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MouseAction {
    pub kind: Act,
    /// Row and column of the first cell.
    pub y: usize,
    pub x: usize,
    pub y2: usize,
    pub x2: usize,
    pub dir: Option<Dir>,
    pub shift: bool,
    pub n: usize,
}

/// Turns a "-- mouse --" body into actions: "click R C", "dblclick R C",
/// "tripleclick R C", "press R C", "release R C", "move R C",
/// "drag R C R C", each optionally with "Shift+",
/// "wheel [Shift+]up|down|left|right [N]" and "tick [N]", rows and
/// columns 0-based.
pub fn parse_mouse(s: &str) -> Result<Vec<MouseAction>, String> {
    let f: Vec<&str> = s.split_whitespace().collect();
    let mut acts = Vec::new();
    let ints = |i: usize, n: usize| -> Result<Vec<usize>, String> {
        if i + n > f.len() {
            return Err(format!("{}: want {n} numbers", f[i - 1]));
        }
        f[i..i + n]
            .iter()
            .map(|t| {
                t.parse()
                    .map_err(|_| format!("{}: {t:?} is not a number", f[i - 1]))
            })
            .collect()
    };
    // An optional repeat count at f[i], defaulting to 1.
    let count = |i: usize| -> (usize, usize) {
        match f.get(i).and_then(|t| t.parse().ok()) {
            Some(n) => (i + 1, n),
            None => (i, 1),
        }
    };
    let mut i = 0;
    while i < f.len() {
        let word = f[i];
        let (name, shift) = match word.strip_prefix("Shift+") {
            Some(rest) => (rest, true),
            None => (word, false),
        };
        let mut a = MouseAction {
            kind: Act::Click,
            y: 0,
            x: 0,
            y2: 0,
            x2: 0,
            dir: None,
            shift,
            n: 0,
        };
        if shift && matches!(name, "wheel" | "tick") {
            return Err(format!("unknown mouse action {word:?}"));
        }
        i += 1;
        a.kind = match name {
            "click" => Act::Click,
            "dblclick" => Act::DblClick,
            "tripleclick" => Act::TripleClick,
            "press" => Act::Press,
            "release" => Act::Release,
            "move" => Act::Move,
            "drag" => Act::Drag,
            "wheel" => Act::Wheel,
            "tick" => Act::Tick,
            _ => return Err(format!("unknown mouse action {name:?}")),
        };
        match a.kind {
            Act::Drag => {
                let v = ints(i, 4)?;
                (a.y, a.x, a.y2, a.x2) = (v[0], v[1], v[2], v[3]);
                i += 4;
            }
            Act::Wheel => {
                let (dir, shift) = match f.get(i).and_then(|d| d.strip_prefix("Shift+")) {
                    Some(rest) => (Some(rest), true),
                    None => (f.get(i).copied(), false),
                };
                a.shift = shift;
                a.dir = Some(match dir {
                    Some("up") => Dir::Up,
                    Some("down") => Dir::Down,
                    Some("left") => Dir::Left,
                    Some("right") => Dir::Right,
                    _ => return Err("wheel: want up, down, left or right".to_string()),
                });
                i += 1;
                (i, a.n) = count(i);
            }
            Act::Tick => (i, a.n) = count(i),
            _ => {
                let v = ints(i, 2)?;
                (a.y, a.x) = (v[0], v[1]);
                i += 2;
            }
        }
        acts.push(a);
    }
    Ok(acts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let acts = parse_mouse(
            "click 1 2 dblclick 3 4 tripleclick 5 6 drag 0 1 2 3 wheel up wheel down 5 wheel left wheel Shift+right 2 click 0 0 press 1 1 release 2 0 move 5 0 tick tick 3 Shift+click 1 2 Shift+drag 0 1 2 3",
        )
        .unwrap();
        let at = |kind, y, x| MouseAction {
            kind,
            y,
            x,
            y2: 0,
            x2: 0,
            dir: None,
            shift: false,
            n: 0,
        };
        let wheel = |dir, n, shift| MouseAction {
            kind: Act::Wheel,
            dir: Some(dir),
            n,
            shift,
            ..at(Act::Wheel, 0, 0)
        };
        let want = [
            at(Act::Click, 1, 2),
            at(Act::DblClick, 3, 4),
            at(Act::TripleClick, 5, 6),
            MouseAction {
                y2: 2,
                x2: 3,
                ..at(Act::Drag, 0, 1)
            },
            wheel(Dir::Up, 1, false),
            wheel(Dir::Down, 5, false),
            wheel(Dir::Left, 1, false),
            wheel(Dir::Right, 2, true),
            at(Act::Click, 0, 0),
            at(Act::Press, 1, 1),
            at(Act::Release, 2, 0),
            at(Act::Move, 5, 0),
            MouseAction {
                n: 1,
                ..at(Act::Tick, 0, 0)
            },
            MouseAction {
                n: 3,
                ..at(Act::Tick, 0, 0)
            },
            MouseAction {
                shift: true,
                ..at(Act::Click, 1, 2)
            },
            MouseAction {
                y2: 2,
                x2: 3,
                shift: true,
                ..at(Act::Drag, 0, 1)
            },
        ];
        assert_eq!(acts, want);
        for bad in [
            "click 1",
            "click a b",
            "drag 1 2 3",
            "wheel sideways",
            "tap 1 1",
            "Shift+tick",
            "Shift+wheel up",
        ] {
            assert!(parse_mouse(bad).is_err(), "{bad:?} should fail");
        }
    }
}
