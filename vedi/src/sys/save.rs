//! The file :write saves to.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::app::Sink;

/// A file being written. One made new is written in place; one that
/// replaces another is written beside it and renamed over it once done,
/// so the old file is whole until then: it may be the input, read as
/// the write goes. Dropped before done, either leaves no file.
pub struct Saving {
    file: File,
    /// The file being written, and where it goes when that differs.
    at: PathBuf,
    dest: Option<PathBuf>,
}

/// Opens `name` for writing: made new, or when `overwrite` replaced
/// once done; `AlreadyExists` otherwise for one that exists.
pub fn open(dest: &Path, overwrite: bool) -> io::Result<Saving> {
    let dest = dest.to_path_buf();
    if !overwrite || !dest.exists() {
        return Ok(Saving {
            file: File::create_new(&dest)?,
            at: dest,
            dest: None,
        });
    }
    // A unique name beside the destination, on its file system, so the
    // rename is one.
    let base = dest
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let at = dest.with_file_name(format!(".{base}.vedi-{}", std::process::id()));
    let file = File::create_new(&at)?;
    if let Ok(perms) = fs::metadata(&dest).map(|m| m.permissions()) {
        let _ = fs::set_permissions(&at, perms);
    }
    Ok(Saving {
        file,
        at,
        dest: Some(dest),
    })
}

impl Write for Saving {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.file.write(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Sink for Saving {
    fn done(mut self: Box<Self>) -> io::Result<()> {
        self.file.flush()?;
        self.file.sync_all()?;
        let Some(dest) = self.dest.take() else {
            // In place: nothing to remove on drop.
            self.at = PathBuf::new();
            return Ok(());
        };
        fs::rename(&self.at, &dest)?;
        self.at = PathBuf::new();
        Ok(())
    }
}

impl Drop for Saving {
    fn drop(&mut self) {
        if self.at != Path::new("") {
            let _ = fs::remove_file(&self.at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Dir {
            static N: AtomicUsize = AtomicUsize::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("vedi-save-{}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(&self.0)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn save(name: &Path, overwrite: bool, data: &[u8]) -> io::Result<()> {
        let mut s = Box::new(open(name, overwrite)?);
        s.write_all(data)?;
        s.done()
    }

    /// A new file is made; one that exists is refused, and replaced only
    /// with overwrite; nothing else is left in the directory.
    #[test]
    fn makes_refuses_replaces() {
        let d = Dir::new();
        let f = d.path("f");
        save(&f, false, b"one").unwrap();
        let err = save(&f, false, b"two").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&f).unwrap(), b"one");
        save(&f, true, b"2").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"2");
        save(&d.path("g"), true, b"new").unwrap();
        assert_eq!(d.names(), ["f", "g"]);
    }

    /// The file replaced is whole until the write is done, so it can be
    /// the input, and keeps its permissions.
    #[test]
    fn replaces_once_done() {
        let d = Dir::new();
        let f = d.path("f");
        fs::write(&f, b"input").unwrap();
        fs::set_permissions(&f, fs::Permissions::from_mode(0o640)).unwrap();
        let mut s = Box::new(open(&f, true).unwrap());
        s.write_all(b"out").unwrap();
        let mut old = String::new();
        File::open(&f).unwrap().read_to_string(&mut old).unwrap();
        assert_eq!(old, "input");
        s.done().unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"out");
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(d.names(), ["f"]);
    }

    /// A save dropped before done leaves the file as it was, or none.
    #[test]
    fn dropped_leaves_nothing() {
        let d = Dir::new();
        let f = d.path("f");
        fs::write(&f, b"input").unwrap();
        let mut s = open(&f, true).unwrap();
        s.write_all(b"partial").unwrap();
        drop(s);
        assert_eq!(fs::read(&f).unwrap(), b"input");
        let mut s = open(&d.path("g"), false).unwrap();
        s.write_all(b"partial").unwrap();
        drop(s);
        assert_eq!(d.names(), ["f"]);
    }
}
