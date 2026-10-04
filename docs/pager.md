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

## The input's name

The status line starts with the input's name: the file, or `<stdin>`.
With several files, the name is the file the cursor is in and its
number among them, `b.txt 2/3`; lines are numbered within it, and
`:N` goes to its line N. A row above each file names it;
`file_separators no` in the config, or `--no-file-separators`,
leaves the rows out.

A program can set the terminal's window title as it writes, with the
escape sequence OSC 0 or 2: `ESC ] 2 ; title BEL`. vedi does not pass
it to the terminal: the title in effect at the cursor's line replaces
`<stdin>` in the status line, and `[` and `]` stop at each line that
sets one, to the same title or to none included.

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

Piped input is read as it is; `--no-open-cmd` reads the files as they
are too, for one run. `open_cmd` in `vedi.conf` sets it for good; see
[Configuration](configuration.md#settings).

## git-delta

[delta](https://github.com/dandavison/delta) is a pager for git that
pipes its colored diffs through a second pager. In `~/.gitconfig`:

    [core]
        pager = delta
    [delta]
        pager = vedi -F

`DELTA_PAGER=vedi -F` in the environment does the same.

`[` and `]` step through delta's hunks and files once it sets a
[window title](#the-inputs-name) on each:

    git config --global delta.hunk-label $'\e]2;hunk\a•'
    git config --global delta.file-transformation $'s,(.*),\e]2;$1\a$1,'

The first titles each hunk `hunk`, the second each file with its
path. git's config has no escape for ESC or BEL; bash's `$'…'` writes
the bytes themselves.
