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
      --clipboard-cmd CMD   pipe copied text to CMD instead of OSC 52

With no files, `vedi` reads stdin.

## Keys

| Key | Action |
|---|---|
| Arrows, Home, End | Move the cursor |
| PgUp, PgDn, Space, b | Move by a page |
| Ctrl+Left/Right | Move by word |
| g, G | First line, last line |
| Shift + arrows, Home, End, PgUp, PgDn; Ctrl+Shift+Left/Right | Extend the selection |
| Ctrl+A | Select all |
| Esc | Clear the selection, then the search highlight |
| Ctrl+C, y | Copy the selection as plain text |
| Enter | With a selection: copy and quit. Without: down one line |
| / | Search (smartcase); n and N for next and previous |
| w | Toggle wrap / nowrap |
| q | Quit |

## Clipboard

Copy uses OSC 52, which works in kitty, xterm, tmux (`set-clipboard on`)
and over ssh. If your terminal does not support it, set
`--clipboard-cmd pbcopy` (macOS), `--clipboard-cmd wl-copy` (Wayland) or
`--clipboard-cmd 'xclip -selection clipboard'` (X11).

## kitty

    scrollback_pager vedi -S +INPUT_LINE_NUMBER

Kitty sends one line per screen row, so `-S` keeps them as they were.

## License

Copyright (C) 2026 Daniel Lee Harple

GNU General Public License, version 3 only; see `LICENSE`.
