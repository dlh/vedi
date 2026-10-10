use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// Begins line i (1-based) and is what the harness waits for. The colon
/// keeps "00000001:" from matching inside a longer number, and is never
/// skipped as a blank cell the way a space is.
pub fn marker(i: usize) -> String {
    format!("{i:08}:")
}

/// The plain text of every line, which follows the marker once the
/// escapes are gone.
pub const FILLER: &str = "some plain text about sixty bold columns wide";

/// Writes n lines to path: a marker, then colored filler, 54 columns in
/// all so nothing wraps at 80.
pub fn write_input(path: &Path, n: usize) -> io::Result<()> {
    let mut w = BufWriter::with_capacity(1 << 20, File::create(path)?);
    for i in 1..=n {
        writeln!(
            w,
            "{}\x1b[32msome plain text about\x1b[0m sixty \x1b[1mbold\x1b[0m columns wide",
            marker(i)
        )?;
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempDir;
    use crate::screen::Screen;

    #[test]
    fn write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("in");
        write_input(&path, 3).unwrap();
        let data = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = data.strip_suffix('\n').unwrap().split('\n').collect();
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines[2].starts_with(&marker(3)), "{lines:?}");
        let mut s = Screen::new(1, 80);
        s.write(lines[0].as_bytes());
        assert_eq!(s.to_string().trim_end().len(), 54);
    }
}
