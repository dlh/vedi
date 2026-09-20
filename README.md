# vedi

See it, select it, copy it. A pager that shows ANSI-colored text and
lets you select it with the keyboard or mouse.

## Install

Download a binary for linux or macOS from the [releases
page](https://github.com/dlh/vedi/releases), or build from source:

    go install go.dlh.dev/vedi/cmd/vedi@latest

## Usage

    vedi [flags] [file...]
      -S, --nowrap            start in nowrap mode
      -F, --quit-if-one-page  print the text and quit if it fits the screen
      +G                      start at the last line and follow until EOF
      +N                      start with line N at the top
      --scrolled-by N         start on the last screenful, scrolled N rows up
      --cursor-row N          put the cursor on row N of the last screenful
      --cursor-col N          put the cursor in column N of the last screenful
      --clipboard-cmd CMD     pipe copied text to CMD instead of OSC 52
      -v, --version           print the version

With no files, `vedi` reads stdin. The status line names the files
being viewed, or `<stdin>`.

`-F` is for a pager that should get out of the way: when all the text
fits on one screen it is printed as if by `cat`, and the pager only
opens for more. Pressing a key while the input is still coming keeps
the pager.

## Keys

| Key | Action |
|---|---|
| ↑ ↓ ← →, Home, End | Move the cursor |
| j, k | Down a line, up a line |
| ⇞ ⇟, Space, f, ⌃F, b, ⌃B | Move by a page |
| ⌃V, ⌥V | Down a page, up a page |
| d, ⌃D, u, ⌃U | Move by half a page |
| ⌃←, ⌃→, ⌥←, ⌥→, ⌥B, ⌥F | Move by word |
| g, G, <, > | First line, last line |
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
| ? | Search backward |
| n, N | Next and previous match; ? swaps them |
| : | Go to a line number |
| w | Toggle wrap / nowrap |
| q | Quit |
| h | Show the key bindings |

The selection is drawn in reverse video and search matches in black
on bright yellow, whatever the text's own colors.

## Clipboard

Copy uses OSC 52, which works in kitty, xterm, tmux (`set-clipboard on`)
and over ssh. Where it does not, set `--clipboard-cmd pbcopy` (macOS),
`--clipboard-cmd wl-copy` (Wayland) or
`--clipboard-cmd 'xclip -selection clipboard'` (X11).

## Terminal.app

Terminal.app has no mapping for ⇧↑, ⇧↓, ⇧Home, ⇧End, ⇧⇞ or ⇧⇟, so
the program sees them as unshifted. To select with them, add these
under Settings → Profiles → Keyboard, action Send Text:

| Key | Text |
|-----|------|
| ⇧↑ | `\033[1;2A` |
| ⇧↓ | `\033[1;2B` |
| ⇧Home | `\033[1;2H` |
| ⇧End | `\033[1;2F` |
| ⇧⇞ | `\033[5;2~` |
| ⇧⇟ | `\033[6;2~` |

It does not support OSC 52, so copy uses `pbcopy`.

## kitty scrollback

The [kitty](https://sw.kovidgoyal.net/kitty/) terminal can hand its
scrollback to a pager. This binding in `kitty.conf` opens it in vedi,
scrolled to the rows you were looking at, with the cursor where it was:

    map <shortcut> launch --type overlay --stdin-source=@screen_scrollback --stdin-add-formatting vedi --scrolled-by @scrolled-by --cursor-row @cursor-y --cursor-col @cursor-x

Or set vedi as the `scrollback_pager`. That gets one line per screen
row — `-S` keeps them so, but a wrapped line then copies with a newline
at each wrap:

    scrollback_pager vedi -S +INPUT_LINE_NUMBER

## bat

[bat](https://github.com/sharkdp/bat) pipes its colored output through
a pager. To make it vedi:

    export BAT_PAGER=vedi

or put `--pager=vedi` in `~/.config/bat/config`.

## License

Copyright (C) 2026 Daniel Lee Harple

GNU General Public License, version 3 only; see `LICENSE`.
