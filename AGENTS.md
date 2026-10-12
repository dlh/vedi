# vedi

A pager: view ANSI-colored text, select with the keyboard, copy.

- `README.md` says what it does; `docs/` says how to use it: keys,
  configuration, and setting it up as the pager.
- `specs/` says exactly how, and enforces it: one plain-text scenario per
  behavior, run by `cargo test -p vedi --test specs`. Format in
  `specs/README.md`.
- A behavior change edits its scenario in the same commit; a new behavior gets a
  new scenario and a line in `README.md` or `docs/`. Expected output is written
  by hand, never generated.
- A default key works in terminals as shipped. One that needs a terminal setting
  is not bound; `docs/development.md` says what terminals send.
- Colors, read errors and the event loop are tested in Rust
  (`vedi/src/app/tests.rs`), as is `open_cmd`, which runs in the binary where no
  spec reaches (`vedi/tests/main.rs`); everything else belongs in `specs/`.
- `vedi/man/vedi.1` is written by hand, in mdoc. A change to a flag, a
  default key, a setting or a `:` command edits it in the same commit, and its
  `.Dd` date; tests check it names each and that `mandoc -T lint` passes.
- `make test` must pass before every commit.
- `docs/development.md` says how to check, commit and release.
- When updating the benchmarks in `docs/comparison.md`, its vedi version is the
  one `make -s next-version` prints, without the `v`; when it fails, the last
  tag's.
- Docs, comments and commit messages are terse: say it once, in as few words as
  read clearly. No preamble, no restating the code.
- A comment says why, or what the code cannot show: a constraint, a case it
  guards against. Never what the code does; delete one that only repeats it.
