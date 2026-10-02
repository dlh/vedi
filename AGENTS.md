# vedi

A pager: view ANSI-colored text, select with the keyboard, copy.

- `README.md` says what it does; `docs/` says how to use it: keys,
  configuration, and setting it up as the pager.
- `specs/` says exactly how, and enforces it: one plain-text scenario
  per behavior, run by `go test ./internal/app/ -run TestSpecs`. Format
  in `specs/README.md`.
- A behavior change edits its scenario in the same commit; a new
  behavior gets a new scenario and a line in `README.md` or `docs/`.
  Expected output is written by hand, never generated.
- Colors, read errors and the event loop are tested in Go
  (`internal/app/app_test.go`); everything else belongs in `specs/`.
- `go test ./...` must pass before every commit.
- `docs/development.md` says how to check, commit and release.
- When updating the benchmarks in `docs/comparison.md`, its vedi
  version is the one `make -s next-version` prints, without the `v`; when it fails, the last
  tag's.
- Docs, comments and commit messages are terse: say it once, in as
  few words as read clearly. No preamble, no restating the code.
