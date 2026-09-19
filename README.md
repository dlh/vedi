# vedi

See it, select it, copy it. A pager that shows ANSI-colored text and
lets you select it with the keyboard.

## Install

    go install go.dlh.dev/vedi/cmd/vedi@latest

## Usage

    vedi [flags] [file...]
      -S, --nowrap          start in nowrap mode
      +G                    start at the last line and follow until EOF
      +N                    start with line N at the top
      --scrolled-by N       start on the last screenful, scrolled N rows up
      --cursor-row N        put the cursor on row N of the last screenful
      --cursor-col N        put the cursor in column N of the last screenful
      --clipboard-cmd CMD   pipe copied text to CMD instead of OSC 52

With no files, `vedi` reads stdin. The status line names the files
being viewed, or `<stdin>`.

## Keys

| Key | Action |
|---|---|
| ↑ ↓ ← →, Home, End | Move the cursor |
| ⇞ ⇟, Space, b | Move by a page |
| ⌃←, ⌃→, ⌥←, ⌥→, ⌥B, ⌥F | Move by word |
| g, G | First line, last line |
| ⇧ + ↑ ↓ ← →, Home, End | Extend the selection |
| ⇧⇞, ⇧⇟ | Extend by a page |
| ⌃⇧←, ⌃⇧→, ⌥⇧←, ⌥⇧→ | Extend by word |
| ⌃A | Select all |
| ⎋ | Clear the selection, then the search highlight |
| ⌃C, y | Copy the selection as plain text |
| ⏎ | Copy the selection and quit; with none, down a line |
| / | Search (smartcase); n and N for next and previous |
| w | Toggle wrap / nowrap |
| q | Quit |
| ? | Show the key bindings |

## Clipboard

Copy uses OSC 52, which works in kitty, xterm, tmux (`set-clipboard on`)
and over ssh. Where it does not, set `--clipboard-cmd pbcopy` (macOS),
`--clipboard-cmd wl-copy` (Wayland) or
`--clipboard-cmd 'xclip -selection clipboard'` (X11).

## kitty

Open the scrollback on the rows you were looking at, cursor where it was:

    map <shortcut> launch --type overlay --stdin-source=@screen_scrollback --stdin-add-formatting vedi --scrolled-by @scrolled-by --cursor-row @cursor-y --cursor-col @cursor-x

Or as the `scrollback_pager`, which gets one line per screen row — `-S`
keeps them so, but a wrapped line then copies with a newline at each wrap:

    scrollback_pager vedi -S +INPUT_LINE_NUMBER

## License

Copyright (C) 2026 Daniel Lee Harple

GNU General Public License, version 3 only; see `LICENSE`.
