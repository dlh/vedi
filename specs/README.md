# Specs

Each file here is one behavior of `vedi` and runs as a test
(`go test ./internal/app/ -run TestSpecs`). The prose at the top says
what the behavior is; the sections below show it. When a behavior
changes, its file changes in the same commit.

## Format

Scenarios live at `specs/<feature>/<behavior>.txt`, one directory deep;
the runner only globs that shape. Free text before the first
`-- name --` line describes the behavior. Sections run in order, so
keys and assertions interleave.

```
Shift+Down extends the selection to the same column on the next line.

-- size --
20x4
-- input --
line 1
line 2
-- keys --
Shift+Down y
-- screen --
[line 1 ]
line 2

copied 1 line
-- cursor --
1 0
-- clipboard --
line 1

```

Setup:

| Section | Content |
|---|---|
| `-- size --` | `WxH`. Default `40x6`. |
| `-- args --` | The command line: `-S +G`, `--scrolled-by 1 --cursor-row 3`, `--clipboard-cmd false`. Split on whitespace, no quoting, so a `--clipboard-cmd` value cannot contain spaces. File names only name the input in the status line; the text still comes from `-- input --`. |
| `-- input --` | Text for the buffer. A section ending with a blank line is input that ends with a newline. Repeats append later. |
| `-- eof --` | Ends the input. Without one, input is complete before the first draw. |

`size` and `args` come before the first key. `-- input --` and
`-- eof --` may also come after keys: that is how `+G` follow, `+N`
waiting for its line and `--cursor-row` waiting for EOF are described;
each later `input` appends and delivers the reader's notification. An
`-- eof --` anywhere in the file keeps the input open from the first
draw, so a file may end with a bare `-- eof --` to say "input was
still being read" (see `startup/follow-stops-on-key.txt`).

Actions:

| Section | Content |
|---|---|
| `-- keys --` | `Up Down Left Right Home End PgUp PgDn Enter Esc Backspace Space`, with `Shift+`, `Ctrl+` and `Alt+`; a single character as itself, optionally with `Alt+`; `"quoted text"` typed rune by rune. |
| `-- mouse --` | `click R C`, `dblclick R C`, `drag R C R C` (press at the first cell, release at the second), `wheel up` or `wheel down`, optionally followed by a count of ticks. `R C` is a screen cell, 0-based like `-- cursor --`. A second `click` is never a double-click: the clock moves on a second before each. |
| `-- resize --` | `WxH`. |

Assertions, checked at that point:

| Section | Content |
|---|---|
| `-- screen --` | Every row. Reverse-video runs — selection, search matches, control characters — in `[` `]`; the status row plain; trailing spaces trimmed. |
| `-- cursor --` | `row col`, 0-based, or `hidden`. |
| `-- clipboard --` | The copied text; a copy ending in a newline ends the section with a blank line. |
| `-- quit --` | The last key quit. Follows the keys that quit; assertions may follow it. |

Colors are not covered; those tests stay in `internal/app/app_test.go`.
