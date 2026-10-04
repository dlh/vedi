# vedi

See it, select it, copy it. A pager for colored terminal output:
select text with the keyboard or mouse, and copy it to the clipboard.

![vedi searching, selecting and copying colored text](docs/media/demo.gif)

## Install

With [Homebrew](https://brew.sh):

    brew install dlh/tap/vedi

Or download a binary for linux or macOS from the [releases
page](https://github.com/dlh/vedi/releases), or build from source:

    go install go.dlh.dev/vedi/cmd/vedi@latest

Tab completion for bash, zsh and fish is a file to install; see
[Shell completion](docs/configuration.md#shell-completion).

## Usage

    vedi [flags] [file...]
      -S, --[no-]wrap              wrap long lines; -S is --no-wrap (default: on)
      --wrap-style STYLE           wrap at the screen's edge (char) or at words (word)
      -F, --[no-]quit-if-one-page  print the text and quit if it fits the screen (default: off)
      --one-page-rows-below N      rows -F leaves below the text, for the prompt (default: 1)
      --[no-]auto-reload           read a file again when it changes on disk (default: on)
      +G                           start at the last line and follow until EOF
      +N                           start with line N at the top
      --scrolled-by N              start on the last screenful, scrolled N rows up
      --cursor-row N               put the cursor on row N of the last screenful
      --cursor-col N               put the cursor in column N of the last screenful
      --clipboard-cmd CMD          pipe copied text to CMD instead of OSC 52
      --no-clipboard-cmd           copy with OSC 52, or pbcopy in Terminal.app
      --open-cmd CMD               read each file by running CMD, %s the file
      --no-open-cmd                read each file as it is
      --tab-width N                draw a tab as N cells (default: 8)
      --[no-]edge-markers          mark text off the sides with < and >, a wrapped row with \ (default: off)
      --[no-]file-separators       with several inputs, draw a row naming each (default: on)
      --config FILE                read the config from FILE, not ~/.config/vedi/vedi.conf
      --completion SHELL           print the completion script for bash, zsh or fish
      -v, --version                print the version

With no files, `vedi` reads stdin. `h` shows the keys and `q` quits;
Shift with the arrows, or the mouse, selects, and Ctrl+c or Cmd+c
copies.

SGR colors and attributes, including 24-bit color, and OSC 8
hyperlinks are drawn as the terminal would. Copy sends plain text.

`:` opens a command prompt, where any setting from the config file
can be changed for the rest of the run.

A file that changes on disk is read again, so a log that grows is
followed.

`-F` is for a pager that should get out of the way: text that fits on
one screen is printed as if by `cat`, and the pager opens only for
more. To make vedi your pager:

    export PAGER="vedi -F"

vedi also reads flags from the `VEDI` environment variable, so they
apply however it is started.

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
