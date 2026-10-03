# vedi as the pager

Programs that page their output, git, man and psql among them, run
the command named by `PAGER`. `-F` suits a pager that should get out
of the way: text that fits on one screen is printed as if by `cat`,
and the pager opens only for more. In `~/.bashrc`, `~/.zshrc` or the
shell's equivalent:

    export PAGER="vedi -F"

## VEDI

vedi reads flags from the `VEDI` environment variable before those on
its command line, so they apply whatever runs it:

    export PAGER=vedi
    export VEDI="-F -S"

A flag on the command line wins over the same one in `VEDI`, and both
win over `vedi.conf`: `vedi --no-quit-if-one-page` opens the pager for
one run despite the `-F` above. A start position on the command line,
`+N`, `+G`, `--scrolled-by` or `--cursor-*`, replaces one in `VEDI`.

The value is split into words as a shell would: quote with `'` or
`"`, or put `\` before a space.

    export VEDI="-F --clipboard-cmd 'xclip -selection clipboard'"

`VEDI` takes flags only: a file name, `-h` or `-v` there is an error.

## git

git reads `GIT_PAGER`, then `core.pager`, then `PAGER`, and colors
what it pages. To set the pager for git alone:

    git config --global core.pager "vedi -F"

## man

man reads `MANPAGER`, then `PAGER`. Its output marks bold and
underline with backspaces, which vedi does not read; `col -bx` strips
them:

    export MANPAGER="sh -c 'col -bx | vedi -F'"

## bat

[bat](https://github.com/sharkdp/bat) reads `BAT_PAGER`, then `PAGER`.
To set the pager for bat alone:

    export BAT_PAGER="vedi -F"

or put `--pager="vedi -F"` in `~/.config/bat/config`.

The other way around, `--open-cmd` reads every file vedi is given
through bat, for syntax coloring:

    export VEDI="-F --open-cmd 'bat --color=always --paging=never %s'"

Piped input is read as it is, and `--no-open-cmd` reads the files so
for one run. `open_cmd` in `vedi.conf` sets it for good; see
[Configuration](configuration.md#settings).

## git-delta

[delta](https://github.com/dandavison/delta) is a pager for git that
pipes its colored diffs through a second pager. In `~/.gitconfig`:

    [core]
        pager = delta
    [delta]
        pager = vedi -F

`DELTA_PAGER=vedi -F` in the environment does the same.

## The input's name

The status line starts with the input's name: the file, or `<stdin>`.
A program that sets the window title with OSC 2 as it writes names
its output: the title in effect at the cursor's line replaces
`<stdin>`. With several files, the name is the file the cursor is in.
