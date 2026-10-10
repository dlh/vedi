//! Runs every scenario under `specs/`, as `specs/README.md` describes
//! them. `VEDI_SPEC=<file>` runs one; with `VEDI_SPEC_STEP=1` it is
//! printed as it runs, section by section.

mod spec;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use spec::archive;
use spec::scenario;
use spec::step::Stepper;

fn scenario_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in fs::read_dir(root).expect("specs dir").flatten() {
        if !dir.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for f in fs::read_dir(dir.path()).expect("spec dir").flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "txt") {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn specs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../specs");
    let files = match env::var("VEDI_SPEC") {
        // A relative path is the repo root's, where bin/spec-step runs.
        Ok(one) => {
            let given = PathBuf::from(&one);
            let rooted = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join(&given);
            vec![if given.is_relative() && rooted.exists() {
                rooted
            } else {
                given
            }]
        }
        Err(_) => scenario_files(&root),
    };
    assert!(!files.is_empty(), "no scenario files found");
    let step = env::var("VEDI_SPEC_STEP").is_ok_and(|v| v == "1");
    let (mut passed, mut failed) = (0, 0);
    for path in &files {
        let name = path.strip_prefix(&root).unwrap_or(path).display();
        let data = fs::read_to_string(path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let a = archive::parse(&data);
        let outcome = if step {
            let mut stepper = Stepper::new(&a);
            scenario::run(&a, Some(&mut stepper))
        } else {
            scenario::run(&a, None)
        };
        match outcome {
            Ok(()) => passed += 1,
            Err(msg) => {
                failed += 1;
                println!("FAIL {name}: {msg}");
            }
        }
    }
    println!("{passed} passed, {failed} failed");
    assert_eq!(failed, 0, "{failed} scenarios failed");
}
