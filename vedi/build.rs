use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=VEDI_VERSION");
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dirs = git_dirs(Path::new(&manifest));
    if let Some((git, common)) = &dirs {
        watch_git(git, common);
    }

    let version = env::var("VEDI_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| dirs.is_some().then(describe).flatten())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=VEDI_VERSION={version}");
}

// A worktree keeps HEAD and index in its own git dir, refs in the common one.
fn git_dirs(manifest: &Path) -> Option<(PathBuf, PathBuf)> {
    let out = git(
        manifest,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ],
    )?;
    let mut lines = out.lines().map(PathBuf::from);
    Some((lines.next()?, lines.next()?))
}

// Cargo reruns on every build for a path that does not exist, so watch only
// those that do: a ref or the tags dir may live only in packed-refs.
fn watch_git(git: &Path, common: &Path) {
    let mut paths = vec![git.join("HEAD"), git.join("index")];
    paths.push(common.join("refs/tags"));
    paths.push(common.join("packed-refs"));
    // HEAD names the branch; a commit moves the branch ref, and the index tracks --dirty.
    if let Some(head) = std::fs::read_to_string(git.join("HEAD"))
        .ok()
        .and_then(|h| h.strip_prefix("ref: ").map(|r| r.trim().to_owned()))
    {
        paths.push(common.join(head));
    }
    for p in paths.iter().filter(|p| p.exists()) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
}

// None when git is missing or fails, so builds from a tarball still succeed.
fn describe() -> Option<String> {
    git(
        Path::new("."),
        &["describe", "--tags", "--always", "--dirty"],
    )
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!v.is_empty()).then_some(v)
}
