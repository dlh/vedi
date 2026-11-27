# vedi as the pager

- [PAGER](#pager)
- [-F](#-f)
- [VEDI](#vedi)
- [Shell completion](#shell-completion)
- [git](#git)
- [man](#man)
- [bat](#bat)
- [git-delta](#git-delta)

## PAGER

Programs that page their output, such as git, man and psql, run the command
named by `PAGER`. In `~/.bashrc`, `~/.zshrc` or the shell's equivalent:

    export PAGER="vedi -F"

A short name helps at the prompt, as in `v log.txt` or `make 2>&1 | v`:

    alias v=vedi

## -F

With `-F`, text that fits on one screen is printed as if by `cat`, and the pager
opens only for more.

What `-F` prints is what the pager would show: the text, its colors and its
links. The text cannot move the cursor or write to the clipboard:

- Any other escape sequence or control character is left out.
- A carriage return inside a line is printed as `^M`.

`-F` leaves one row below the text for the shell's prompt. With a taller prompt
the first lines may scroll off, so say how many rows it takes:

    export PAGER="vedi -F --one-page-rows-below 2"

Without a terminal, `-F` measures against `COLUMNS` and `LINES`, else 80×25, and
prints what fits.

## VEDI

vedi reads flags from the `VEDI` environment variable, so they apply whatever
runs it:

    export PAGER=vedi
    export VEDI="-F -S"

A flag on the command line wins over the same flag in `VEDI`, so
`vedi --no-quit-if-one-page` opens the pager for one run despite the `-F` above.
[Settings](configuration.md#settings) has the full order.

A start position on the command line, `+N`, `+G`, `--scrolled-by` or
`--cursor-*`, replaces one in `VEDI`.

The value is split into words as a shell would. Quote with `'` or `"`, or put
`\` before a space:

    export VEDI="-F --clipboard-cmd 'xclip -selection clipboard'"

`VEDI` takes flags only. A file name, `-h` or `-v` there is an error.

## Shell completion

Tab completes vedi's flags and their values in bash, zsh and fish. Homebrew and
the AUR packages install the scripts. A release archive holds them in
`completions/`.

`vedi --completion SHELL` prints the script for a shell. Put it where the shell
looks:

    # bash
    vedi --completion bash > ~/.local/share/bash-completion/completions/vedi

    # zsh; the directory must be in $fpath before compinit runs
    vedi --completion zsh > ~/.zfunc/_vedi

    # fish
    vedi --completion fish > ~/.config/fish/completions/vedi.fish

## git

git reads `GIT_PAGER`, then `core.pager`, then `PAGER`, and colors what it
pages. To set the pager for git alone:

    git config --global core.pager "vedi -F"

## man

man reads `MANPAGER`, then `PAGER`. Its output marks bold and underline with
backspaces (overstrike), which vedi does not read. `col -bx` strips them:

    export MANPAGER="sh -c 'col -bx | vedi -F'"

## bat

To view files with [bat](https://github.com/sharkdp/bat)'s highlighting, have
vedi run bat rather than bat run vedi. In `vedi.conf`:

    open_cmd bat --color=always --paging=never %s

`vedi file.rs` then opens the file itself, which piped text cannot give:

- The status line names the file.
- `R` and auto-reload run bat again when the file changes.
- `O` switches between bat's output and the file as it is.

[open_cmd](configuration.md#open_cmd) can also choose a command by file type.

To page what bat prints when it is run by itself, set vedi as its pager. bat
reads `BAT_PAGER`, then `PAGER`:

    export BAT_PAGER="vedi -F"

`--pager="vedi -F"` in `~/.config/bat/config` also sets it.

## git-delta

[delta](https://github.com/dandavison/delta) is a pager for git that pipes its
colored diffs through a second pager. Set both, and have delta title each hunk
and file:

    git config --global core.pager delta
    git config --global delta.pager "vedi -F"
    git config --global delta.hunk-label $'\e]2;hunk\a•'
    git config --global delta.file-transformation $'s,(.*),\e]2;$1\a$1,'

| Setting | Does |
|---|---|
| `core.pager` | git pages through delta. |
| `delta.pager` | delta pages through vedi. |
| `delta.hunk-label` | Sets the [window title](usage.md#titles) `hunk` on each hunk. |
| `delta.file-transformation` | Sets the window title on each file to its path. |

With the titles set, `[` and `]` step through delta's hunks and files.

git's config has no escape for ESC or BEL; bash's `$'…'` writes the bytes
themselves.
