# vedi

See it, select it, copy it. A pager for colored terminal output: select text
with the keyboard or mouse, and copy it to the clipboard.

![vedi searching, selecting and copying colored text](docs/media/demo.gif)

<details>
<summary>The filter feature</summary>

![vedi filtering a source file and copying a function](docs/media/filter.gif)

</details>

<details>
<summary>The hints feature</summary>

![vedi jumping to a paragraph by hint and selecting it](docs/media/hint.gif)

</details>

## Features

- **[Select and copy](docs/usage.md#selecting-and-copying).** Shift with the
  arrows selects, and Ctrl+c or Cmd+c copies. The mouse selects too, and copies
  when the button comes up.
- **[Search](docs/usage.md#search).** `/` searches as you type.
- **[Filter](docs/usage.md#filter).** `&` lists only the matching lines. Jump to
  a match, or select and copy from the list.
- **[Hints](docs/usage.md#hints).** `s` labels each match or line; type a label
  to jump there.
- **Colors.** SGR colors and attributes, including 24-bit color, OSC 8
  hyperlinks, and CSI cursor forward and repeat are drawn as the terminal would.
- **Reload.** A file that changes on disk is read again.
- **[Commands](docs/commands.md).** `:` changes any setting for the rest of the
  run, and `:write FILE` saves the text or the selection.
- **[Themes](docs/themes.md).** Each color vedi draws itself, from the selection
  to the status line, can be set, and vedi ships several themes.

## Install

**Homebrew**

    brew install dlh/tap/vedi

**Arch Linux**, from the [AUR](https://aur.archlinux.org/packages/vedi), or as
[vedi-bin](https://aur.archlinux.org/packages/vedi-bin) to skip the build:

    git clone https://aur.archlinux.org/vedi.git && cd vedi && makepkg -si

**A binary** for linux or macOS is on the
[releases page](https://github.com/dlh/vedi/releases).
[Verify it](SECURITY.md#verifying-a-download).

**From source**

    cargo install --git https://github.com/dlh/vedi vedi

Homebrew and the AUR packages install the man page and tab completion for bash,
zsh and fish. An archive holds them in `man/` and `completions/`; see
[Shell completion](docs/pager.md#shell-completion).

## Usage

    vedi [flags] [file...]
    command | vedi

With no files, vedi reads stdin. `h` shows the keys and `q` quits.

To make vedi your pager:

    export PAGER="vedi -F"

`-F` gets the pager out of the way: text that fits on one screen is printed as
if by `cat`, and the pager opens only for more.

vedi also reads flags from the `VEDI` environment variable, so they apply
however it is started.

### Flags

    vedi [flags] [file...]
      -S, --[no-]wrap              wrap long lines; -S is --no-wrap (default: on)
      --wrap-style STYLE           wrap at the screen's edge (char) or at words (word)
      --wrap-width N               wrap rows at N cells when the screen is wider (default: off)
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
      --[no-]copy-on-select        copy what the mouse selects when the button comes up (default: on)
      --open-cmd CMD               read each file by running CMD, %s the file
      --no-open-cmd                read each file as it is
      --tab-width N                draw a tab as N cells (default: 8)
      --[no-]edge-markers          mark text off the sides with < and >, a wrapped row with \ (default: off)
      --[no-]file-separators       with several inputs, draw a row naming each (default: on)
      --[no-]end-markers           start each row below the text with ~ (default: on)
      --[no-]status-line           draw the status line; off, only for a prompt or a message (default: on)
      --view-style STYLE           draw the text in color, without (plain) or with its escapes as text (raw)
      --hint-order ORDER           number hints from the top (top_down) or the bottom (bottom_up)
      --[no-]line-numbers          number the lines (default: off)
      --line-numbers-side SIDE     number them at the left or right edge of the screen (default: left)
      --config FILE                read the config from FILE, not ~/.config/vedi/vedi.conf
      --completion SHELL           print the completion script for bash, zsh or fish
      -h, --help                   show this help
      -v, --version                print the version

## Documentation

| Doc | Covers |
|---|---|
| [Using vedi](docs/usage.md) | The cursor, selecting, search, hints, the filter and several files |
| [Keys](docs/keys.md) | Every default key |
| [Commands](docs/commands.md) | What the `:` prompt takes, `:write` included |
| [Configuration](docs/configuration.md) | `vedi.conf`: settings, `open_cmd`, colors and rebinding |
| [Themes](docs/themes.md) | A screenshot of each theme vedi ships |
| [Pager](docs/pager.md) | vedi as the pager for the shell, git, man, bat and delta; shell completion |
| [Terminals](docs/terminals.md) | The clipboard, Terminal.app's keys, Option as Alt and kitty's scrollback |
| [Comparison](docs/comparison.md) | How vedi differs from less, moor, ov, tmux, vim and neovim, with benchmarks |
| [Development](docs/development.md) | Checks, commits and releases |
| [Decisions](docs/decisions.md) | Why vedi is the way it is, and what it chose not to be |

## License

Copyright (C) 2026 Daniel Lee Harple

GNU General Public License, version 3 only; see `LICENSE`.

Release archives include `LICENSE-THIRD-PARTY`.
