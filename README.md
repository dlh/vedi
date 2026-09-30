# vedi

See it, select it, copy it. A pager for colored terminal output:
select text with the keyboard or mouse, and copy it to the clipboard.

![vedi paging colored lorem ipsum: a search lights up its matches, three lines are selected and copied with Ctrl+c, and pbpaste prints them as plain text](docs/media/demo.gif)

## Install

Download a binary for linux or macOS from the [releases
page](https://github.com/dlh/vedi/releases), or build from source:

    go install go.dlh.dev/vedi/cmd/vedi@latest

## Usage

    vedi [flags] [file...]
      -S, --nowrap            start in nowrap mode
      --wrap                  start in wrap mode (the default)
      -F, --quit-if-one-page  print the text and quit if it fits the screen
      --auto-reload           read a file again when it changes on disk (the default)
      --no-auto-reload        leave a changed file as it was read; R still reloads
      +G                      start at the last line and follow until EOF
      +N                      start with line N at the top
      --scrolled-by N         start on the last screenful, scrolled N rows up
      --cursor-row N          put the cursor on row N of the last screenful
      --cursor-col N          put the cursor in column N of the last screenful
      --clipboard-cmd CMD     pipe copied text to CMD instead of OSC 52
      --tab-width N           draw a tab as N cells (8)
      --edge-markers          in nowrap mode, mark text off the sides with < and >
      --no-edge-markers       leave the edges bare (the default)
      --config FILE           read the config from FILE, not ~/.config/vedi/vedi.conf
      -v, --version           print the version

With no files, `vedi` reads stdin. `h` shows the keys and `q` quits;
Shift with the arrows, or the mouse, selects, and Ctrl+c or Cmd+c
copies.

SGR colors and attributes, including 24-bit color, and OSC 8
hyperlinks are drawn as the terminal would. Copy sends plain text.

A file that changes on disk is read again, so a log that grows is
followed. `-F` is for a pager that should get out of the way: text
that fits on one screen is printed as if by `cat`, and the pager
opens only for more. To make vedi your pager:

    export PAGER="vedi -F"

## Documentation

- [Keys](docs/keys.md): every key, for moving, selecting, copying and searching
- [Configuration](docs/configuration.md): `vedi.conf`, settings and rebinding
- [Pager](docs/pager.md): vedi as the pager for the shell, git, man, bat and delta
- [Terminals](docs/terminals.md): the clipboard, Terminal.app's Shift keys and kitty's scrollback
- [Comparison](docs/comparison.md): how vedi differs from less, moor, ov, tmux, vim and neovim, with benchmarks
- [Development](docs/development.md): checks, commits and releases

## License

Copyright (C) 2026 Daniel Lee Harple

GNU General Public License, version 3 only; see `LICENSE`.
