# vedi

A pager: view ANSI-colored text, select with the keyboard, copy.

- `README.md` says what it does.
- `specs/` says exactly how, and enforces it: one plain-text scenario
  per behavior, run by `go test ./internal/app/ -run TestSpecs`. Format
  in `specs/README.md`.
- A behavior change edits its scenario in the same commit; a new
  behavior gets a new scenario and a README line. Expected output is
  written by hand, never generated.
- Colors, read errors and the event loop are tested in Go
  (`internal/app/app_test.go`); everything else belongs in `specs/`.
- `go test ./...` must pass before every commit.
- `docs/development.md` says how to check, commit and release.
- Docs, comments and commit messages are terse: say it once, in as
  few words as read clearly. No preamble, no restating the code.
