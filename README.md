# vedi

See it, select it, copy it. A pager that shows ANSI-colored text and
lets you select it with the keyboard or mouse.

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
| Click, drag | Move the cursor, select |
| Double-click | Select the word |
| Wheel | Scroll |
| ⌃A | Select all |
| ⎋ | Clear the selection, then the search highlight |
| ⌃C, y | Copy the selection as plain text |
| ⏎ | Copy the selection and quit; with none, down a line |
| / | Search; ignores case unless the pattern has a capital |
| n, N | Next and previous match |
| : | Go to a line number |
| w | Toggle wrap / nowrap |
| q | Quit |
| ? | Show the key bindings |

## Clipboard

Copy uses OSC 52, which works in kitty, xterm, tmux (`set-clipboard on`)
and over ssh. Where it does not, set `--clipboard-cmd pbcopy` (macOS),
`--clipboard-cmd wl-copy` (Wayland) or
`--clipboard-cmd 'xclip -selection clipboard'` (X11).

## kitty scrollback

The [kitty](https://sw.kovidgoyal.net/kitty/) terminal can hand its
scrollback to a pager. This binding in `kitty.conf` opens it in vedi,
scrolled to the rows you were looking at, with the cursor where it was:

    map <shortcut> launch --type overlay --stdin-source=@screen_scrollback --stdin-add-formatting vedi --scrolled-by @scrolled-by --cursor-row @cursor-y --cursor-col @cursor-x

Or set vedi as the `scrollback_pager`. That gets one line per screen
row — `-S` keeps them so, but a wrapped line then copies with a newline
at each wrap:

    scrollback_pager vedi -S +INPUT_LINE_NUMBER

## License

Copyright (C) 2026 Daniel Lee Harple

GNU General Public License, version 3 only; see `LICENSE`.
