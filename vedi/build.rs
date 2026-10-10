use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=VEDI_VERSION");
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let git = Path::new(&manifest).join("../.git");
    println!("cargo:rerun-if-changed={}", git.join("HEAD").display());
    println!("cargo:rerun-if-changed={}", git.join("refs/tags").display());
    // HEAD names the branch; a commit moves the branch ref, and the index tracks --dirty.
    if let Some(head) = std::fs::read_to_string(git.join("HEAD"))
        .ok()
        .and_then(|h| h.strip_prefix("ref: ").map(|r| r.trim().to_owned()))
    {
        println!("cargo:rerun-if-changed={}", git.join(head).display());
    }
    println!("cargo:rerun-if-changed={}", git.join("index").display());

    let version = env::var("VEDI_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| git.exists().then(describe).flatten())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=VEDI_VERSION={version}");
}

// None when git is missing or fails, so builds from a tarball still succeed.
fn describe() -> Option<String> {
    let out = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!v.is_empty()).then_some(v)
}
