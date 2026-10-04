# Specs

Each file here is one behavior of `vedi` and runs as a test
(`go test ./internal/app/ -run TestSpecs`). The prose at the top says
what the behavior is; the sections below show it. When a behavior
changes, its file changes in the same commit.

## Stepping through one

    bin/spec-step specs/mouse/click-moves-cursor.txt

prints each section as it runs and the screen it leaves, in a frame
the scenario's size, with the cursor's cell underlined. It waits for
Enter before each action; `q` runs to the end, as does stdin that is
not a terminal.

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
| `-- env --` | The `VEDI` environment variable: `-S --tab-width 2`, `--clipboard-cmd 'sh -c false'`. Split as vedi splits it, quotes and all. |
| `-- os --` | `macos` or `linux`, for the keys that exist on one and not the other. Default `linux`. |
| `-- config --` | A config file body, laid over the default bindings: `map h left`, `map q none`, `wrap no`, `clipboard_cmd sh -c false`, `tab_width 4`. |
| `-- input --` | Text for the buffer. A section ending with a blank line is input that ends with a newline. Repeats append later. |
| `-- next-file --` | Input from here on comes from the next name in `-- args --`, for the status line's name. |
| `-- eof --` | Ends the input. Without one, input is complete before the first draw. |

`size`, `args`, `env`, `os` and `config` come before the first key. `-- input --` and
`-- eof --` may also come after keys: that is how `+G` follow, `+N`
waiting for its line and `--cursor-row` waiting for EOF are described;
each later `input` appends and delivers the reader's notification. An
`-- eof --` anywhere in the file keeps the input open from the first
draw, so a file may end with a bare `-- eof --` to say "input was
still being read" (see `startup/follow-stops-on-key.txt`).

With `-F` the pager opens only once the text is known not to fit,
as `main` opens it: an `input` that outgrows the screen opens it, and
an `eof` with the text still fitting quits to print instead. Until
one or the other there is no screen to act on or assert about, and
a section that tries fails.

Actions:

| Section | Content |
|---|---|
| `-- keys --` | `Up Down Left Right Home End PgUp PgDn Enter Esc Tab Backspace Space`, with `Shift+`, `Ctrl+`, `Alt+` and `Cmd+`; a single character as itself, optionally with `Alt+`, `Cmd+` or `Cmd+Shift+`; `"quoted text"` typed rune by rune. |
| `-- mouse --` | `click R C`, `dblclick R C`, `tripleclick R C`, `drag R C R C` (press at the first cell, release at the second), `press R C` and `release R C` (the halves of a click, so keys can come between), `move R C` (motion with the button held), each optionally with `Shift+`, `wheel up`, `wheel down`, `wheel left` or `wheel right`, optionally with `Shift+` on the direction and followed by a count of ticks, `tick` (the auto-scroll timer armed by the last motion fires), optionally followed by a count. `R C` is a screen cell, 0-based like `-- cursor --`. A second `click` is never a double-click: the clock moves on a second before each. |
| `-- resize --` | `WxH`. |
| `-- file --` | The file on disk now holds this text, ended the way `-- input --` is. Needs a file in `-- args --`; disk starts as the input, each file's from its `-- input --` sections. With several files the section names the one that changed: `-- file b.txt --`. |
| `-- reload --` | The same, and the app is told the file changed: unless `--no-auto-reload`, `auto_reload no` or `:auto_reload no` is in effect, the reload lands before the next section. |

Assertions, checked at that point:

| Section | Content |
|---|---|
| `-- screen --` | Every row. Reverse-video runs — selection, control characters — in `[` `]`; search matches in `{` `}`, those the cursor is in in `{{` `}}`; the status row plain, with its runs out of reverse video in `[` `]`; trailing spaces trimmed. |
| `-- cursor --` | `row col`, 0-based, or `hidden`. |
| `-- clipboard --` | The copied text; a copy ending in a newline ends the section with a blank line. |
| `-- quit --` | The app quit. Follows the section that quit it — keys, or input or eof for `-F`; assertions may follow it, unless `-F` printed the text and the pager never opened. |

Colors are not covered; those tests stay in `internal/app/app_test.go`.
