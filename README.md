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
      --auto-reload           read a file again when it changes on disk (the default)
      --no-auto-reload        leave a changed file as it was read; R still reloads
      +G                      start at the last line and follow until EOF
      +N                      start with line N at the top
      --scrolled-by N         start on the last screenful, scrolled N rows up
      --cursor-row N          put the cursor on row N of the last screenful
      --cursor-col N          put the cursor in column N of the last screenful
      --clipboard-cmd CMD     pipe copied text to CMD instead of OSC 52
      --config FILE           read key bindings from FILE, not ~/.config/vedi/vedi.conf
      -v, --version           print the version

With no files, `vedi` reads stdin. The status line names the files
being viewed, or `<stdin>`.

A file that changes on disk is read again, the cursor keeping its
line, or the last line if it was on it: a log that grows is followed.
`R` reads it again by hand. A pipe is read once. `--no-auto-reload`,
or `auto_reload no` in the config, leaves a changed file to `R`.

`-F` is for a pager that should get out of the way: when all the text
fits on one screen it is printed as if by `cat`, and the pager only
opens for more. Pressing a key while the input is still coming keeps
the pager.

## Keys

| Key | Action |
|---|---|
| h | Show the key bindings |
| q | Quit |
| w | Toggle wrap / nowrap |
| R | Reload the file |
| Up, Down, Left, Right, Home, End, j, k, Cmd+Left, Cmd+Right | Move the cursor |
| PgDn, Space, f, Ctrl+f, Ctrl+v | Down a page |
| PgUp, b, Ctrl+b, Alt+v | Up a page |
| d, Ctrl+d | Down half a page |
| u, Ctrl+u | Up half a page |
| Ctrl+Left, Ctrl+Right, Alt+Left, Alt+Right, Alt+b, Alt+f | Move by word |
| g, G, <, >, Cmd+Up, Cmd+Down | First line, last line |
| : | Go to a line number |
| J, K, Shift+Up, Shift+Down, Shift+Left, Shift+Right, Shift+Home, Shift+End, Cmd+Shift+Left, Cmd+Shift+Right, Cmd+Shift+Up, Cmd+Shift+Down | Extend the selection |
| Shift+PgUp, Shift+PgDn | Extend by a page |
| Ctrl+Shift+Left, Ctrl+Shift+Right, Alt+Shift+Left, Alt+Shift+Right | Extend by word |
| Ctrl+a, Cmd+a | Select all |
| Esc | Clear the selection, then the search highlight |
| Click, drag | Move the cursor, select; held at an edge, scroll |
| Double-, triple-click | Select the word, the line |
| Wheel | Scroll |
| Ctrl+c, Cmd+c, y | Copy the selection as plain text |
| Enter | Copy the selection and quit; with none, down a line |
| / | Search; ignores case if lowercase; empty repeats |
| ? | Search backward |
| Up, Down at the prompt | Recall earlier searches |
| n, N, Cmd+g, Cmd+Shift+g | Next and previous match; ? swaps them |

The Cmd+ keys are macOS only, and reach vedi where the terminal passes
them on: kitty does, Cmd+c when it has no selection of its own;
Terminal.app keeps them.

Any key can be rebound in `~/.config/vedi/vedi.conf`, or the file
named by `--config`, and `clear_all_shortcuts` there starts the map
from nothing. The `set_mark` action, unbound by default, starts a
selection that every motion extends, as `v` does in vim; `h` shows
the bindings in effect, and the motions scroll them when they do not
fit. See [docs/configuration.md](docs/configuration.md).

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

    export BAT_PAGER="vedi -F"

or put `--pager="vedi -F"` in `~/.config/bat/config`.

## git-delta

[delta](https://github.com/dandavison/delta) is a pager for git that
pipes its colored diffs through a second pager. In `~/.gitconfig`:

    [core]
        pager = delta
    [delta]
        pager = vedi -F

`DELTA_PAGER=vedi -F` in the environment does the same.

## License

Copyright (C) 2026 Daniel Lee Harple

GNU General Public License, version 3 only; see `LICENSE`.
