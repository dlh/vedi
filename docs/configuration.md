# Configuration

- [The file](#the-file)
- [Settings](#settings)
- [Colors](#colors)
- [Commands](#commands)
- [Keys](#keys)
- [Actions](#actions)
- [Defaults](#defaults)
- [vim](#vim)
- [emacs](#emacs)
- [Shell completion](#shell-completion)

## The file

vedi reads `~/.config/vedi/vedi.conf` (or `$XDG_CONFIG_HOME/vedi/vedi.conf`), or
the file named by `--config`. Without a file it uses the [defaults](#defaults).
A bad line stops vedi before it opens, naming the file and line.

One directive per line. Blank lines and `#` comments are skipped.

    # vim-style horizontal motion
    map h left
    map l right
    map Alt+h help

    # or, from nothing
    clear_all_shortcuts
    map q quit
    map j down
    map k up

`map <key> <action>` binds a key. Later lines win over earlier ones and over the
defaults; the action `none` unbinds. `clear_all_shortcuts` drops every binding
so far, defaults included.

`include FILE` reads another file there, as if its lines were in this one. A
leading `~` is the home directory; a relative name is from this file's
directory.

## Settings

| Setting | Default | Flags | Effect |
| --- | --- | --- | --- |
| `auto_reload` | `yes` | `--auto-reload`, `--no-auto-reload` | Reread a file that changes on disk. `R` reloads either way. |
| `wrap` | `yes` | `--wrap`, `-S` | Start in wrap mode. `w` toggles. |
| `wrap_style` | `char` | `--wrap-style` | `word` breaks rows at spaces and tabs. |
| `wrap_width` | `0` | `--wrap-width` | Wrap rows at this many cells when the screen is wider; `0` wraps at its edge. |
| `tab_width` | `8` | `--tab-width` | Cells per tab stop. |
| `edge_markers` | `no` | `--edge-markers`, `--no-edge-markers` | Mark text that runs off the screen. |
| `file_separators` | `yes` | `--file-separators`, `--no-file-separators` | Name each input in a row when several are given; see [the input's name](pager.md#the-inputs-name). |
| `end_markers` | `yes` | `--end-markers`, `--no-end-markers` | Start each row below the text with `~`. |
| `status_line` | `yes` | `--status-line`, `--no-status-line` | Draw the status line. |
| `view_style` | `color` | `--view-style` | `plain` draws the text without its colors; `raw` shows its escapes. |
| `hint_order` | `top_down` | `--hint-order` | `bottom_up` numbers `s`'s hints from the bottom of the screen. |
| `line_numbers` | `no` | `--line-numbers`, `--no-line-numbers` | Number the lines. |
| `line_numbers_side` | `left` | `--line-numbers-side` | `right` puts the numbers at the screen's right edge. |
| `one_page_rows_below` | `1` | `--one-page-rows-below` | Rows `-F` leaves below the text, for the shell's prompt. |
| `copy_on_select` | `yes` | `--copy-on-select`, `--no-copy-on-select` | Copy what the mouse selects when the button comes up. |
| `clipboard_cmd` | | `--clipboard-cmd`, `--no-clipboard-cmd` | Pipe copied text to a command instead of OSC 52. |
| `open_cmd` | | `--open-cmd`, `--no-open-cmd` | Read each file through a command, by name pattern or for all; `%s` is the file name. |

Flags apply for one run. Flags in the `VEDI` environment variable override the
file, and the command line overrides `VEDI`; see [Pager](pager.md#vedi).

**wrap_style word.** A word that does not fit moves to the next row, taking the
space after it; one wider than the screen breaks at the edge. The status line
shows `word wrap`.

**wrap_width 80.** Rows wrap at 80 cells, a dim `│` in the 81st. With
`edge_markers yes` the `\` takes the 80th. `W` switches between `0` and `80`.

**edge_markers yes.** In nowrap mode a row that runs past the right edge ends in
`>`, and one scrolled off the left edge starts with `<`, both in reverse video.
In wrap mode a row that continues on the next ends in `\`, and text wraps a
column early to leave room for it.

**status_line no.** The text has the bottom row. The line comes back for a
prompt, a message, help or a read error, and goes with the next key. A copy the
mouse makes says nothing unless it fails, so the row stays under the next click.

**view_style plain.** The text is drawn in the terminal's default colors,
without its SGR colors and attributes; links stay, and `-F` prints the same. The
status line shows `plain`.

**view_style raw.** The text is the input's bytes: an escape sequence is drawn
as its characters, the ESC as `^[` in reverse video, and colors nothing. Search
finds its text and copy sends it. No title names stdin, and `[` and `]` stop
only at inputs. Changing to or from `raw` keeps the cursor's line and clamps its
column. `-F` prints the escapes spelled out. The status line shows `raw`.

**line_numbers yes.** Each line's first row has its number, counted in its
input as the status line counts, in a column as wide as the text's line count; a
wrapped line's other rows have none. The text has the columns left. On the
right the numbers are at the screen's edge, or after the `wrap_width` bar when
there is one. In the filter's list a line has the number it has in the text.
Help shows none, nor does a screen too narrow to leave the text a column, and
`-F` prints the text without them.

**copy_on_select no.** Releasing the mouse button no longer copies what it
selected; the copy keys do. By default it does, as a terminal copies what the
mouse selects when no program has taken the mouse.

**clipboard_cmd** and **open_cmd** take the rest of the line as the command:
`clipboard_cmd xclip -selection clipboard`,
`open_cmd bat --color=always --paging=never %s`. A pipeline needs a script.

**clipboard_cmd** runs in the background. The status line says `copying` until
it is done, and `copy_and_quit` waits for it. A second copy before then ends it,
as does quitting.

**open_cmd** leaves stdin as it is. A command that fails ends the text with a
read error naming its exit status and stderr; `R` and auto-reload run it again.

**open_cmd by file type.** A glob before the command limits it to files whose
name matches, case ignored. A glob is a word with `*`, `?` or `[`; `[!a-c]`
excludes, and `[*]` is a literal star:

    open_cmd *.md mdcat --ansi
    open_cmd --no-fallback *.json jq -C . %s
    open_cmd bat --color=always --paging=never %s

A file is read through the last line whose pattern matches it, else the last
fallback line, one without a pattern. A command without `%s` gets the file name
as its last argument; a name starting with `-` is passed as `./-name`, so the
command does not take it for an option. `--open-cmd` replaces every line for
one run.

`O` (`cycle open_cmd`) steps each file through every line that matches it, then
reads it as it is. Above, a `.md` file goes from mdcat to bat to as it is.
`--no-fallback` before a pattern keeps the fallback lines from the files it
matches, so a `.json` file goes from jq to as it is. The status line names the
command for the file at the cursor, or `''` for none, and the selection is
cleared.

## Colors

`color <element> <style>` sets how vedi draws one of its own elements. A theme
is a file of `color` lines: `include themes/NAME.conf`.

Homebrew installs example themes in `$(brew --prefix)/share/vedi/themes/`, the
AUR packages in `/usr/share/vedi/themes/`, and a release archive holds them in
`themes/`. Include one from there, or copy it to `~/.config/vedi/themes/` to
change it. [themes.md](themes.md) shows each.

    color selection      on #3a3d4a
    color match          black on #e5c07b
    color current_match  black on #e06c75 bold
    color status         white on 238

| Element | Default | Draws |
| --- | --- | --- |
| `selection` | `reverse` | The selection. |
| `match` | `black on bright_yellow` | Search matches. |
| `current_match` | `bright_white on magenta` | The match the cursor is in. |
| `hint` | `black on bright_cyan bold` | `s`'s labels. |
| `note` | `black on yellow` | A message saying what was done. |
| `error` | `bright_white on red` | One saying what could not be. |
| `status` | `reverse` | The status line, and a completion Tab fills in. |
| `edge_marker` | `reverse` | `<`, `>` and `\` with `edge_markers yes`. |
| `control` | `reverse` | A control character, drawn as `^X`. |
| `guide` | `dim` | The `wrap_width` bar and the `~` below the text. |
| `separator` | `cyan` | The row naming each input. |
| `separator_name` | `bold` | The name on it. |
| `line_number` | `dim` | The numbers of `line_numbers`. |

A style is `[color] [on color] [attribute...]`: the foreground, the background,
then any of `bold`, `dim`, `italic`, `underline`, `reverse` and `strike`. A
color is `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan` or
`white`, a `bright_` one, a number from `0` to `255`, `#rrggbb`, or `default`,
the terminal's own. The named colors and numbers are the terminal's palette, so
they follow its theme; `#rrggbb` does not.

A line replaces the element's whole style: `color match on blue` is the default
foreground on blue, not bold. A `control` style is drawn over the text's own,
which shows where it sets no color.

## Commands

`:` opens a prompt on the status line. A bad command is an error there.

| Command | Effect |
| --- | --- |
| `:<setting> <value>` | Change a setting for the rest of the run: `:wrap_style word`, `:tab_width 4`. Any setting above but `clipboard_cmd`, `open_cmd` and `one_page_rows_below`. |
| `:map <key> <action>` | Bind a key, as in the file. `clear_all_shortcuts` is refused: it would unbind `:` and `q`. |
| `:color <element> <style>` | Set a [color](#colors), as in the file. |
| `:22`, `:goto 22` | Go to line 22. |
| `:cycle <setting>` | Step a setting to its next value, wrapping to the first. Takes `wrap`, `wrap_style`, `edge_markers`, `file_separators`, `end_markers`, `auto_reload`, `status_line`, `view_style`, `copy_on_select`, `hint_order`, `line_numbers`, `line_numbers_side`
and `open_cmd`. |
| `:cycle <setting> <value>...` | Step through the values given instead: the one after the current, or the first when the current is not among them. Any setting the prompt takes, with two or more different values: `:cycle tab_width 4 8`. |
| `:help`, `:reload`, `:quit` | What `h`, `R` and `q` do. |
| `:write FILE` | Save the text, or the selection when there is one, to a new FILE as plain text, as copy sends it. |
| `:write --raw FILE` | Save the input itself, byte for byte; a selection's lines whole. |
| `:write --color FILE` | Save what the screen shows, with its colors and links as SGR and OSC 8. |
| `:write! FILE` | The same, replacing a FILE that exists, the file being viewed included. |

**write.** The file name is the rest of the line; a leading `~` is the home
directory. A symlink you own is written through; another user's is refused.

**Editing.** Tab completes the word: a command, a setting's value (starting
after the current one) or `write`'s file name. Up and Down recall earlier
commands; Esc, Ctrl+g and Ctrl+c cancel. [keys.md](keys.md) lists the rest of
the prompt keys. A paste goes in as one line, its newlines dropped, here and at
the search prompt.

## Keys

A key is one of
`Up Down Left Right Home End PgUp PgDn Enter Esc Tab Backspace Space`, or a
single character, with any of `Shift+`, `Ctrl+`, `Alt+`, `Cmd+` in front: `q`,
`Ctrl+f`, `Alt+Shift+Left`, `Ctrl+Space`.

- `Ctrl+f` and `Ctrl+F` are the same key.
- A shifted character is just that character: `G`, not `Shift+g`. Only `Cmd+`
  takes `Shift+` on a character, and `Cmd+G` is `Cmd+Shift+g`. `Shift+Space` is
  not a key a terminal can send.
- `Cmd+` is Super off macOS. It reaches vedi only where the terminal passes it
  on.
- Under the kitty keyboard protocol a shifted character with `Alt+` or `Cmd+`,
  as `Alt+<`, is read as on a US layout.

`h` shows the bindings in effect; a key that cycles a setting through given
values is on a row naming them.

## Actions

The [defaults](#defaults) below name every action but these:

- Each movement has a `select_` twin that extends the selection instead of
  clearing it: `left` and `select_left`, `page_down` and `select_page_down`.
- `set_mark` starts a selection that every motion then extends until `set_mark`
  again, `clear_selection` or a click ends it: vim's `v`, emacs's `Ctrl+Space`.
- `cycle <setting> [value...]` steps a setting, as in [Commands](#commands). It
  is the one action that takes an argument.

## Defaults

```
# vedi's default bindings: "map <key> <action>", later lines winning.
# Cmd+ is Super off macOS. See docs/configuration.md.

# Move the cursor
map Up up
map Down down
map Left left
map Right right
map Home home
map End end
map j down
map k up
map 0 home
map $ end
map Enter down
map Cmd+Left home
map Cmd+Right end

# Down a page
map PgDn page_down
map Space page_down
map f page_down
map Ctrl+f page_down
map Ctrl+v page_down

# Up a page
map PgUp page_up
map b page_up
map Ctrl+b page_up
map Alt+v page_up

# Down half a page
map d half_page_down
map Ctrl+d half_page_down

# Up half a page
map u half_page_up
map Ctrl+u half_page_up

# Move by word
map Ctrl+Left word_left
map Ctrl+Right word_right
map Alt+Left word_left
map Alt+Right word_right
map Alt+b word_left
map Alt+f word_right

# Move by paragraph
map Alt+Up paragraph_up
map Alt+Down paragraph_down

# Start, end of the text
map g first
map G last
map < first
map > last
map Ctrl+Home first
map Ctrl+End last
map Cmd+Up first
map Cmd+Down last

# Top, middle, bottom of the shown text
map T screen_top
map M screen_middle
map B screen_bottom

# Jump to a match or line by hint
map s hint

# Previous, next input or title
map [ prev_file
map ] next_file

# Extend the selection
map J select_down
map K select_up
map Shift+Up select_up
map Shift+Down select_down
map Shift+Left select_left
map Shift+Right select_right
map Shift+Home select_home
map Shift+End select_end
map Ctrl+Shift+Home select_first
map Ctrl+Shift+End select_last
map Cmd+Shift+Left select_home
map Cmd+Shift+Right select_end
map Cmd+Shift+Up select_first
map Cmd+Shift+Down select_last

# Extend by a page
map Shift+PgUp select_page_up
map Shift+PgDn select_page_down

# Extend by half a page: select_half_page_up, select_half_page_down;
# unbound by default

# Extend to the top, middle, bottom row of the screen: select_screen_top,
# select_screen_middle, select_screen_bottom; unbound by default

# Start a selection that motions extend; again, end it: set_mark, as
# v in vim or Ctrl+Space in emacs; unbound by default

# Extend by word
map Ctrl+Shift+Left select_word_left
map Ctrl+Shift+Right select_word_right
map Alt+Shift+Left select_word_left
map Alt+Shift+Right select_word_right

# Extend by paragraph
map Alt+Shift+Up select_paragraph_up
map Alt+Shift+Down select_paragraph_down

# Select all
map Ctrl+a select_all
map Cmd+a select_all

# Clear the selection, then the search highlight
map Esc clear_selection

# Copy the selection as plain text
map Ctrl+c copy
map Cmd+c copy
map y copy

# Copy the selection and quit; with none, down a line: copy_and_quit;
# unbound by default

# Search as typed; ignores case if lowercase; empty repeats
map / search

# Search backward
map ? search_back

# Next and previous match; ? swaps them
map n search_next
map N search_prev
map Cmd+g search_next
map Cmd+Shift+g search_prev

# Filter: list the lines that match; Enter goes to one
map & filter

# Command prompt; N goes to line N
map : command

# Toggle wrap / nowrap
map w cycle wrap

# Wrap at 80 cells / the screen's edge
map W cycle wrap_width 0 80

# Toggle the line numbers
map # cycle line_numbers

# Line numbers at the left / right edge: cycle line_numbers_side;
# unbound by default

# Reload the file
map R reload

# Read through the next open_cmd
map O cycle open_cmd

# Quit
map q quit

# Show the key bindings
map h help
```

## vim

```
# h and l move; help moves to Alt+h
map h left
map Alt+h help
map l right
# w and b move by word; wrap moves to Alt+w, and Ctrl+b still pages up
map w word_right
map Alt+w cycle wrap
map b word_left
# Shifted, the same keys extend the selection
map H select_left
map L select_right
map W select_word_right
map B select_word_left
# { and } move by paragraph
map { paragraph_up
map } paragraph_down
# v starts a selection that motions extend, and ends it
map v set_mark
```

## emacs

```
# Ctrl+n and Ctrl+p move by line
map Ctrl+n down
map Ctrl+p up
# Ctrl+f and Ctrl+b move by character; Ctrl+v and Alt+v still page
map Ctrl+f right
map Ctrl+b left
# Ctrl+a and Ctrl+e go to the line's ends; Ctrl+a no longer selects all
map Ctrl+a home
map Ctrl+e end
# Alt+< and Alt+> go to the first and last line
map Alt+< first
map Alt+> last
# Ctrl+s and Ctrl+r search; Ctrl+s needs `stty -ixon` in most terminals
map Ctrl+s search
map Ctrl+r search_back
# Alt+w copies; Ctrl+g clears the selection, like Esc
map Alt+w copy
map Ctrl+g clear_selection
# Ctrl+Space sets the mark: motions extend the selection until Ctrl+g
map Ctrl+Space set_mark
```

## Shell completion

Tab completes vedi's flags and their values in bash, zsh and fish. A release
archive holds the scripts in `completions/`; `vedi --completion SHELL` prints
the same ones. Put one where the shell looks:

    # bash, with the bash-completion package
    vedi --completion bash > ~/.local/share/bash-completion/completions/vedi

    # zsh; the directory must be in $fpath before compinit runs
    vedi --completion zsh > ~/.zfunc/_vedi

    # fish
    vedi --completion fish > ~/.config/fish/completions/vedi.fish

Bash without bash-completion reads no directory; save the script anywhere and
source it from `~/.bashrc`.
