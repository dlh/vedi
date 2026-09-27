# vedi as the pager

Programs that page their output, git, man and psql among them, run
the command named by `PAGER`. `-F` suits a pager that should get out
of the way: text that fits on one screen is printed as if by `cat`,
and the pager opens only for more. In `~/.bashrc`, `~/.zshrc` or the
shell's equivalent:

    export PAGER="vedi -F"

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

## git-delta

[delta](https://github.com/dandavison/delta) is a pager for git that
pipes its colored diffs through a second pager. In `~/.gitconfig`:

    [core]
        pager = delta
    [delta]
        pager = vedi -F

`DELTA_PAGER=vedi -F` in the environment does the same.
