//! Tells when files change on disk.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

const EVERY: Duration = Duration::from_secs(1);
const EVERY_WITHOUT_NOTIFY: Duration = Duration::from_millis(500);

/// Calls `changed` when a named file's size or mtime differs from its
/// last stat. Every file is checked every second, and on any `notify`
/// event for it; the watch is on the file, and is put back when an
/// editor's rename-replace takes its inode away. Without `notify` the
/// check runs every half second. A file that cannot be statted keeps
/// its last stamp: one that vanishes fires nothing, one that comes
/// back fires if it came back different. Drop returns once a check
/// under way is over: `changed` is not called after it.
pub struct Watch {
    stop: Arc<AtomicBool>,
    wake: Sender<Msg>,
    thread: Option<JoinHandle<()>>,
}

enum Msg {
    Event(notify::Event),
    Wake,
}

#[derive(Clone, Copy)]
enum Notify {
    Try,
    #[cfg(test)]
    Skip,
}

impl Watch {
    pub fn files(names: &[OsString], changed: impl FnMut() + Send + 'static) -> Watch {
        Watch::spawn(names, changed, Notify::Try, None, || {})
    }

    /// `every` overrides the default tick; `ready` is called once
    /// `notify` watches.
    fn spawn(
        names: &[OsString],
        changed: impl FnMut() + Send + 'static,
        notify: Notify,
        every: Option<Duration>,
        ready: impl FnOnce() + Send + 'static,
    ) -> Watch {
        // Stamped now, so a change once this returns is one.
        let files: Vec<File> = names.iter().map(|n| File::new(n)).collect();
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let mut w = Checker {
            files,
            changed,
            stop: stop.clone(),
        };
        let events = tx.clone();
        // notify is started on the thread: on kqueue, or for FSEvents'
        // run loop, it can take a while.
        let thread = thread::Builder::new()
            .name("vedi-watch".into())
            .spawn(move || {
                let fsw = match notify {
                    Notify::Try => w.notifier(events),
                    #[cfg(test)]
                    Notify::Skip => None,
                };
                ready();
                let every = every.unwrap_or(match fsw {
                    Some(_) => EVERY,
                    None => EVERY_WITHOUT_NOTIFY,
                });
                w.run(fsw, every, rx);
            })
            .expect("spawn watch thread");
        Watch {
            stop,
            wake: tx,
            thread: Some(thread),
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.wake.send(Msg::Wake);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What a check compares.
#[derive(PartialEq, Eq, Clone, Copy)]
struct Stamp {
    size: u64,
    mtime: Option<SystemTime>,
}

fn stat(name: &Path) -> Option<Stamp> {
    let md = fs::metadata(name).ok()?;
    Some(Stamp {
        size: md.len(),
        mtime: md.modified().ok(),
    })
}

struct File {
    name: PathBuf,
    /// The name with links resolved, as FSEvents reports it.
    real: Option<PathBuf>,
    last: Option<Stamp>,
    /// The watch on the file is gone, or never was: it is added again.
    lost: bool,
}

impl File {
    fn new(name: &OsStr) -> File {
        let name: PathBuf = Path::new(name).components().collect();
        File {
            last: stat(&name),
            name,
            real: None,
            lost: false,
        }
    }

    fn is(&self, p: &Path) -> bool {
        p == self.name || Some(p) == self.real.as_deref()
    }
}

struct Checker<F> {
    files: Vec<File>,
    changed: F,
    stop: Arc<AtomicBool>,
}

impl<F: FnMut()> Checker<F> {
    /// The watcher sending to `tx`, or `None` when `notify` cannot
    /// start. A file that cannot be watched is tried again on every
    /// check.
    fn notifier(&mut self, tx: Sender<Msg>) -> Option<RecommendedWatcher> {
        let mut fsw = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                // An error, an overflow say, is dropped: the tick still
                // runs.
                if let Ok(ev) = res {
                    let _ = tx.send(Msg::Event(ev));
                }
            },
            notify::Config::default(),
        )
        .ok()?;
        for f in &mut self.files {
            f.lost = true;
        }
        self.rewatch(&mut fsw);
        Some(fsw)
    }

    /// Adds every file whose watch is gone, fresh: an inode renamed
    /// away may still be watched under the name. All in one batch, and
    /// none when nothing is lost: FSEvents rebuilds its stream, slowly,
    /// per call and per batch.
    fn rewatch(&mut self, fsw: &mut RecommendedWatcher) {
        if !self.files.iter().any(|f| f.lost) {
            return;
        }
        let mut paths = fsw.paths_mut();
        for f in self.files.iter_mut().filter(|f| f.lost) {
            let _ = paths.remove(&f.name);
            if paths.add(&f.name, RecursiveMode::NonRecursive).is_ok() {
                f.lost = false;
                f.real = fs::canonicalize(&f.name).ok();
            }
        }
        let _ = paths.commit();
    }

    /// Checks on every tick and on every event for a file, until
    /// stopped. An event that takes a file's inode away marks its
    /// watch lost. A tick or event that comes with the stop loses to
    /// it.
    fn run(&mut self, mut fsw: Option<RecommendedWatcher>, every: Duration, rx: Receiver<Msg>) {
        let mut next = Instant::now() + every;
        loop {
            let msg = rx.recv_timeout(next.saturating_duration_since(Instant::now()));
            if self.stop.load(Ordering::SeqCst) {
                return;
            }
            match msg {
                Err(RecvTimeoutError::Timeout) => next = Instant::now() + every,
                Err(RecvTimeoutError::Disconnected) => return,
                Ok(Msg::Wake) => continue,
                Ok(Msg::Event(ev)) => {
                    let gone = matches!(
                        ev.kind,
                        EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
                    );
                    let mut ours = false;
                    for f in &mut self.files {
                        if ev.paths.iter().any(|p| f.is(p)) {
                            ours = true;
                            f.lost |= gone;
                        }
                    }
                    if !ours {
                        continue;
                    }
                }
            }
            if let Some(fsw) = &mut fsw {
                self.rewatch(fsw);
            }
            self.check();
        }
    }

    /// Stats every file and calls `changed` once if any differs.
    fn check(&mut self) {
        let mut fired = false;
        for f in &mut self.files {
            if let Some(s) = stat(&f.name)
                && Some(s) != f.last
            {
                f.last = Some(s);
                fired = true;
            }
        }
        if fired {
            (self.changed)();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File, FileTimes};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
    use std::time::{Duration, Instant, SystemTime};

    use super::*;

    /// A directory removed on drop.
    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Dir {
            static N: AtomicUsize = AtomicUsize::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("vedi-watch-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }

        fn file(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().to_string()
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A watch on `names`, started, with a channel that gets a value
    /// per change.
    fn start(names: &[&String], notify: Notify, every: Option<Duration>) -> (Watch, Receiver<()>) {
        let names: Vec<OsString> = names.iter().map(OsString::from).collect();
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let changed = move || {
            let _ = tx.send(());
        };
        let w = Watch::spawn(&names, changed, notify, every, move || {
            let _ = ready_tx.send(());
        });
        // A change before the watch is up would be seen by the tick
        // alone, too late for a long one.
        assert!(fired(&ready), "the watch did not start");
        (w, rx)
    }

    /// Long enough for the parallel tests: a process starts FSEvents
    /// streams one at a time, a third of a second each, and every
    /// watch test starts one.
    fn fired(rx: &Receiver<()>) -> bool {
        rx.recv_timeout(Duration::from_secs(10)).is_ok()
    }

    /// Nothing fires for longer than the slowest check.
    fn quiet(rx: &Receiver<()>) -> bool {
        rx.recv_timeout(Duration::from_millis(1200)) == Err(RecvTimeoutError::Timeout)
    }

    fn set_mtime(name: &str, t: SystemTime) {
        let f = File::options().write(true).open(name).unwrap();
        f.set_times(FileTimes::new().set_accessed(t).set_modified(t))
            .unwrap();
    }

    /// Moves a file's mtime an hour back. Linux stamps mtimes from a
    /// coarse clock, so a same-sized file written over it moments later
    /// would otherwise get the same stamp.
    fn age(name: &str) {
        set_mtime(name, SystemTime::now() - Duration::from_secs(3600));
    }

    #[test]
    fn write_fires() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        let (_w, rx) = start(&[&name], Notify::Try, None);
        fs::write(&name, "ab\n").unwrap();
        assert!(fired(&rx), "a write did not fire");
    }

    #[test]
    fn rename_replace_fires() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        age(&name);
        let (_w, rx) = start(&[&name], Notify::Try, None);
        let tmp = d.file("f.tmp");
        fs::write(&tmp, "b\n").unwrap();
        fs::rename(&tmp, &name).unwrap();
        assert!(fired(&rx), "a rename over the file did not fire");
    }

    #[test]
    fn touch_fires() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        let (_w, rx) = start(&[&name], Notify::Try, None);
        set_mtime(&name, SystemTime::now() + Duration::from_secs(3600));
        assert!(fired(&rx), "a changed mtime did not fire");
    }

    #[test]
    fn vanish_fires_nothing() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        let (_w, rx) = start(&[&name], Notify::Try, None);
        fs::remove_file(&name).unwrap();
        assert!(quiet(&rx), "removing the file fired");
        fs::write(&name, "abc\n").unwrap();
        assert!(fired(&rx), "the file coming back different did not fire");
    }

    #[test]
    fn two_files_one_dir() {
        let d = Dir::new();
        let (a, b) = (d.file("a"), d.file("b"));
        fs::write(&a, "a\n").unwrap();
        fs::write(&b, "b\n").unwrap();
        let (_w, rx) = start(&[&a, &b], Notify::Try, None);
        fs::write(&b, "bb\n").unwrap();
        assert!(fired(&rx), "a write to the second file did not fire");
    }

    #[test]
    fn drop_stops() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        let (w, rx) = start(&[&name], Notify::Try, None);
        drop(w);
        fs::write(&name, "ab\n").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(1200)).is_err(),
            "fired after drop"
        );
    }

    // The tick finds a change with no notify watcher, as when a
    // directory cannot be watched.
    #[test]
    fn polling_alone() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        let (_w, rx) = start(&[&name], Notify::Skip, Some(Duration::from_millis(10)));
        fs::write(&name, "ab\n").unwrap();
        assert!(fired(&rx), "the tick did not find the write");
    }

    // Watching a file in a big directory must not hold up the caller,
    // as watching the directory would on kqueue.
    #[test]
    fn files_returns_at_once() {
        let d = Dir::new();
        for i in 0..4000 {
            fs::write(d.file(&format!("f{i}")), "").unwrap();
        }
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        let before = Instant::now();
        let w = Watch::files(&[name.into()], || {});
        let took = before.elapsed();
        drop(w);
        assert!(took < Duration::from_millis(20), "files took {took:?}");
    }

    /// How many file descriptors the process holds.
    fn open_fds() -> usize {
        fs::read_dir("/dev/fd").unwrap().count()
    }

    // The watch is on the file, not on its directory, which on kqueue
    // would open every sibling.
    #[test]
    fn watches_the_file_alone() {
        let d = Dir::new();
        for i in 0..200 {
            fs::write(d.file(&format!("sibling{i}")), "").unwrap();
        }
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        let before = open_fds();
        let (_w, rx) = start(&[&name], Notify::Try, None);
        fs::write(&name, "ab\n").unwrap();
        assert!(fired(&rx), "a write did not fire");
        // Other tests open descriptors meanwhile; 200 siblings would be
        // far over.
        let got = open_fds().saturating_sub(before);
        assert!(got < 100, "watching one file opened {got} descriptors");
    }

    // After an editor's rename-replace the new file is watched too, so
    // its next write is seen by an event, not by the tick, which here
    // would take an hour.
    #[test]
    fn rename_replace_rewatches() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        age(&name);
        let (_w, rx) = start(&[&name], Notify::Try, Some(Duration::from_secs(3600)));
        let tmp = d.file("f.tmp");
        fs::write(&tmp, "b\n").unwrap();
        fs::rename(&tmp, &name).unwrap();
        assert!(fired(&rx), "the rename over the file did not fire");
        fs::write(&name, "bbb\n").unwrap();
        assert!(fired(&rx), "a write to the new file did not fire");
    }

    // Drop returns only once a check under way is over, so nothing
    // fires after it.
    #[test]
    fn drop_waits_for_check() {
        let d = Dir::new();
        let name = d.file("f");
        fs::write(&name, "a\n").unwrap();
        age(&name);
        let (entered_tx, entered) = mpsc::channel();
        let (release_tx, release) = mpsc::channel::<()>();
        let w = Watch::files(&[OsString::from(&name)], move || {
            let _ = entered_tx.send(());
            let _ = release.recv();
        });
        fs::write(&name, "ab\n").unwrap();
        assert!(fired(&entered), "a write did not fire");
        let (stopped_tx, stopped) = mpsc::channel();
        std::thread::spawn(move || {
            drop(w);
            let _ = stopped_tx.send(());
        });
        assert!(
            stopped.recv_timeout(Duration::from_millis(100)).is_err(),
            "drop returned during a check"
        );
        drop(release_tx);
        assert!(fired(&stopped), "drop did not return after the check");
    }
}
