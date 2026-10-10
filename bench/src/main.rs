//! Compares vedi with less, moor, ov and vim on a large file: time to
//! the first screen, to the end and back, for a failed search, to the
//! first screen from stdin, and peak memory.

mod input;
mod pager;
mod scenario;
mod screen;

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use scenario::{Bench, PAGERS, Row, Sample};

const USAGE: &str = "usage: bench [flags]
  --lines N       lines of input (default 1000000)
  --plugins DIR   a directory of plugin checkouts; vim runs when given
  --runs N        runs per pager; the median is reported (default 5)
  --timeout SECS  give up on a screen after this long (default 60)
  --vedi PATH     the vedi binary (default ./vedi)
";

#[derive(Debug, PartialEq)]
struct Args {
    vedi: PathBuf,
    lines: usize,
    runs: usize,
    timeout: Duration,
    plugins: Option<PathBuf>,
}

/// Why the command line was not run.
#[derive(Debug, PartialEq)]
enum Usage {
    Help,
    Bad(String),
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(Usage::Help) => {
            eprint!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(Usage::Bad(why)) => {
            eprint!("{why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match bench(&args) {
        Ok(results) => {
            let mut out = io::stdout().lock();
            match report(&mut out, &results).and_then(|()| out.flush()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => fatal(e),
            }
        }
        Err(e) => fatal(e),
    }
}

fn fatal(e: io::Error) -> ExitCode {
    eprintln!("bench: {e}");
    ExitCode::FAILURE
}

fn bench(args: &Args) -> io::Result<HashMap<&'static str, Vec<Sample>>> {
    let dir = TempDir::new()?;
    let file = dir.path().join("input.txt");
    input::write_input(&file, args.lines)?;
    let bench = Bench {
        vedi: args.vedi.clone(),
        plugins: args.plugins.clone(),
        timeout: args.timeout,
    };
    let mut results: HashMap<&str, Vec<Sample>> = HashMap::new();
    for i in 1..=args.runs {
        for p in &PAGERS {
            if p.needs_plugins() && args.plugins.is_none() {
                continue;
            }
            eprintln!("{} run {i}/{}", p.name, args.runs);
            let sample = bench.run(p, &file, args.lines);
            results.entry(p.name).or_default().push(sample);
        }
    }
    Ok(results)
}

/// Takes `--name value` and `--name=value`, up to the first argument
/// that is not a flag.
fn parse(argv: impl IntoIterator<Item = String>) -> Result<Args, Usage> {
    let mut args = Args {
        vedi: "./vedi".into(),
        lines: 1_000_000,
        runs: 5,
        timeout: Duration::from_secs(60),
        plugins: None,
    };
    let mut argv = argv.into_iter();
    while let Some(arg) = argv.next() {
        if arg == "-h" || arg == "--help" {
            return Err(Usage::Help);
        }
        let Some(flag) = arg.strip_prefix("--") else {
            break;
        };
        if flag.is_empty() {
            break;
        }
        let (name, value) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        if !["vedi", "lines", "runs", "timeout", "plugins"].contains(&name) {
            return Err(Usage::Bad(format!("unknown flag --{name}")));
        }
        let value = value
            .or_else(|| argv.next())
            .ok_or_else(|| Usage::Bad(format!("--{name} needs a value")))?;
        let bad = || Usage::Bad(format!("bad --{name} {value:?}"));
        match name {
            "vedi" => args.vedi = value.into(),
            "plugins" => {
                args.plugins = Some(value.clone())
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            }
            "lines" => args.lines = value.parse().map_err(|_| bad())?,
            "runs" => args.runs = value.parse().map_err(|_| bad())?,
            _ => args.timeout = Duration::from_secs(value.parse().map_err(|_| bad())?),
        }
    }
    Ok(args)
}

/// Prints a row per measurement and a column per pager: the median over
/// runs, or the first error. Columns are two spaces apart.
fn report(w: &mut impl Write, results: &HashMap<&str, Vec<Sample>>) -> io::Result<()> {
    let ran: Vec<_> = PAGERS
        .iter()
        .filter_map(|p| results.get(p.name).map(|s| (p.name, s)))
        .collect();
    let mut table = vec![
        std::iter::once(String::new())
            .chain(ran.iter().map(|(name, _)| name.to_string()))
            .collect::<Vec<_>>(),
    ];
    for row in Row::ALL {
        table.push(
            std::iter::once(row.label().to_string())
                .chain(ran.iter().map(|(_, samples)| summarize(samples, row)))
                .collect(),
        );
    }
    let cols = table[0].len();
    let width: Vec<usize> = (0..cols)
        .map(|c| {
            table
                .iter()
                .map(|r| r[c].chars().count())
                .max()
                .unwrap_or(0)
                + 2
        })
        .collect();
    for r in &table {
        let mut line = String::new();
        for (c, cell) in r.iter().enumerate() {
            line.push_str(cell);
            if c + 1 < cols {
                let pad = width[c] - cell.chars().count();
                line.extend(std::iter::repeat_n(' ', pad));
            }
        }
        writeln!(w, "{line}")?;
    }
    Ok(())
}

fn summarize(samples: &[Sample], row: Row) -> String {
    let mut vs = Vec::new();
    for s in samples {
        match s.get(&row) {
            None => {}
            Some(Err(e)) => return e.to_string(),
            Some(Ok(v)) => vs.push(*v),
        }
    }
    if vs.is_empty() {
        return "-".into();
    }
    vs.sort_by(f64::total_cmp);
    format!("{:.1}", vs[vs.len() / 2])
}

/// A directory removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> io::Result<Self> {
        static N: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let name = format!(
            "vedi-bench-{}-{}-{nanos}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir(&path)?;
        Ok(TempDir(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pager::Error;

    fn args(a: &[&str]) -> Result<Args, Usage> {
        parse(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn report_table() {
        let one = |c| Sample::from([(Row::End, c)]);
        let results = HashMap::from([
            ("vedi", vec![one(Ok(3.0)), one(Ok(1.0)), one(Ok(2.0))]),
            ("less", vec![one(Err(Error::Timeout))]),
        ]);
        let mut out = Vec::new();
        report(&mut out, &results).unwrap();
        let out = String::from_utf8(out).unwrap();
        let want = "                      vedi  less
first-screen ms       -     -
end ms                2.0   timeout
home ms               -     -
search-miss ms        -     -
stdin ms              -     -
search-miss stdin ms  -     -
rss file MB           -     -
rss stdin MB          -     -
";
        assert_eq!(out, want);
    }

    #[test]
    fn flags() {
        let got = args(&[
            "--vedi",
            "v",
            "--lines=10",
            "--runs",
            "1",
            "--timeout=90",
            "--plugins",
            "p",
        ]);
        assert_eq!(
            got,
            Ok(Args {
                vedi: "v".into(),
                lines: 10,
                runs: 1,
                timeout: Duration::from_secs(90),
                plugins: Some("p".into()),
            })
        );
        let defaults = args(&[]).unwrap();
        assert_eq!(defaults.vedi, Path::new("./vedi"));
        assert_eq!((defaults.lines, defaults.runs), (1_000_000, 5));
        assert_eq!(defaults.timeout, Duration::from_secs(60));
        assert_eq!(defaults.plugins, None);
        assert_eq!(args(&["-h"]), Err(Usage::Help));
        assert_eq!(args(&["--x"]), Err(Usage::Bad("unknown flag --x".into())));
        assert_eq!(
            args(&["--lines"]),
            Err(Usage::Bad("--lines needs a value".into()))
        );
        assert_eq!(
            args(&["--runs", "a"]),
            Err(Usage::Bad(r#"bad --runs "a""#.into()))
        );
    }
}
