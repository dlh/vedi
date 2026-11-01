//! The file :write saves to.

#![allow(unsafe_code)]

use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::MetadataExt;
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
/// once done; `AlreadyExists` otherwise for one that exists. A symlink
/// of the user's own is written through; another's is refused.
pub fn open(dest: &Path, overwrite: bool) -> io::Result<Saving> {
    let me = unsafe { libc::geteuid() };
    let (dest, old) = resolve(dest, me)?;
    let Some(old) = old.filter(|_| overwrite) else {
        return Ok(Saving {
            file: File::create_new(&dest)?,
            at: dest,
            dest: None,
        });
    };
    // A unique name beside the destination, on its file system, so the
    // rename is one.
    let base = dest
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let at = dest.with_file_name(format!(".{base}.vedi-{}", std::process::id()));
    let file = File::create_new(&at)?;
    // Only a file of the user's own lends its mode: another's was set
    // by whoever put it there. On the handle: by path, a symlink
    // swapped in for the file would carry the permissions to whatever
    // it points at.
    if old.uid() == me {
        let _ = file.set_permissions(old.permissions());
    }
    Ok(Saving {
        file,
        at,
        dest: Some(dest),
    })
}

/// The file `name` means, through symlinks the user `me` owns, with
/// what is there, if anything. Another's link is refused: followed, it
/// would write wherever they chose; replaced, the user may have meant
/// its target. So is one where another could swap it for theirs
/// between the check and the read.
fn resolve(name: &Path, me: u32) -> io::Result<(PathBuf, Option<fs::Metadata>)> {
    let refused = |why| io::Error::new(ErrorKind::PermissionDenied, why);
    let mut dest = name.to_path_buf();
    for _ in 0..40 {
        let md = match fs::symlink_metadata(&dest) {
            Ok(md) => md,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok((dest, None)),
            Err(e) => return Err(e),
        };
        if !md.is_symlink() {
            return Ok((dest, Some(md)));
        }
        if md.uid() != me {
            return Err(refused("a symlink another user made"));
        }
        // Entries are replaced by whoever can write the directory, or
        // in a sticky one by their owner and the directory's.
        let dir = match dest.parent() {
            Some(p) if !p.as_os_str().is_empty() => fs::metadata(p)?,
            _ => fs::metadata(".")?,
        };
        let mode = dir.mode();
        #[allow(
            clippy::unnecessary_cast,
            reason = "mode_t is u16 on macOS, u32 on Linux"
        )]
        let sticky = mode & libc::S_ISVTX as u32 != 0;
        if !(dir.uid() == me && mode & 0o022 == 0 || sticky && (dir.uid() == me || dir.uid() == 0))
        {
            return Err(refused("a symlink in a directory others can write"));
        }
        let target = fs::read_link(&dest)?;
        dest = dest.parent().unwrap_or(Path::new("")).join(target);
    }
    Err(io::Error::other("too many symlinks"))
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

    /// A symlink of the user's own is written through: its target gets
    /// the text and keeps its mode, the link stays; one pointing nowhere
    /// gets its target made. The target is what exists, or not.
    #[test]
    fn writes_through_own_symlink() {
        let d = Dir::new();
        let target = d.path("target");
        fs::write(&target, b"old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let link = d.path("link");
        std::os::unix::fs::symlink("target", &link).unwrap();
        let err = save(&link, false, b"new").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        save(&link, true, b"new").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::metadata(&target).unwrap().mode() & 0o777, 0o640);
        let dangling = d.path("dangling");
        std::os::unix::fs::symlink("made", &dangling).unwrap();
        save(&dangling, false, b"filled").unwrap();
        assert!(fs::symlink_metadata(&dangling).unwrap().is_symlink());
        assert_eq!(fs::read(d.path("made")).unwrap(), b"filled");
        assert_eq!(d.names(), ["dangling", "link", "made", "target"]);
    }

    /// A symlink another user made is refused, followed or not; so is
    /// one in a directory others can write, unless sticky; so is a loop.
    #[test]
    fn refuses_anothers_symlink() {
        let d = Dir::new();
        let link = d.path("link");
        std::os::unix::fs::symlink(d.path("target"), &link).unwrap();
        let me = fs::symlink_metadata(&link).unwrap().uid();
        let err = resolve(&link, me.wrapping_add(1)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(err.to_string(), "a symlink another user made");
        let (dest, md) = resolve(&link, me).unwrap();
        assert_eq!(dest, d.path("target"));
        assert!(md.is_none());
        fs::set_permissions(&d.0, fs::Permissions::from_mode(0o777)).unwrap();
        let err = resolve(&link, me).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(err.to_string(), "a symlink in a directory others can write");
        fs::set_permissions(&d.0, fs::Permissions::from_mode(0o1777)).unwrap();
        assert_eq!(resolve(&link, me).unwrap().0, d.path("target"));
        fs::set_permissions(&d.0, fs::Permissions::from_mode(0o755)).unwrap();
        let a = d.path("a");
        std::os::unix::fs::symlink("b", &a).unwrap();
        std::os::unix::fs::symlink("a", d.path("b")).unwrap();
        let err = resolve(&a, me).unwrap_err();
        assert_eq!(err.to_string(), "too many symlinks");
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
