# Configuration

- [Settings](#settings)
- [Commands](#commands)
- [Keys](#keys)
- [Actions](#actions)
- [Defaults](#defaults)
- [vim](#vim)
- [emacs](#emacs)

vedi reads `$XDG_CONFIG_HOME/vedi/vedi.conf`, or
`~/.config/vedi/vedi.conf` when `XDG_CONFIG_HOME` is unset. Without
the file it uses the defaults below. `--config FILE` reads FILE
instead, and then it must exist. A bad line stops vedi before it
opens, naming the file and line.

One directive per line. `map <key> <action>` binds a key; a later
line wins over an earlier one and over the defaults, and the action
`none` unbinds. `clear_all_shortcuts` drops every binding so far, the
defaults included, for a map built from nothing by the lines after
it. Blank lines and lines starting with `#` are skipped.

    # vim-style horizontal motion
    map h left
    map l right
    map Alt+h help

    # or, from nothing
    clear_all_shortcuts
    map q quit
    map j down
    map k up

## Settings

`auto_reload no` leaves a file that changes on disk as it was read;
`R` still reloads it. `auto_reload yes` is the default. On the command
line `--no-auto-reload` and `--auto-reload` override it for one run.

`wrap no` starts in nowrap mode; `w` still toggles. `wrap yes` is the
default. `-S` and `--wrap` override it for one run.

`wrap_style word` makes wrap mode break rows at spaces and tabs, not
at the screen's edge: a word that does not fit moves to the next row
with the space after it, and one wider than the screen breaks where
the row ends. The status line then shows `word wrap`. `wrap_style char` is
the default. `--wrap-style` overrides it for one run.

`clipboard_cmd <command>` pipes copied text to the command instead of
OSC 52; the command is the rest of the line, so `clipboard_cmd xclip
-selection clipboard` works. `--clipboard-cmd` overrides it for one
run, and `--no-clipboard-cmd` copies with OSC 52 for one run.

`open_cmd <command>` reads each file by running the command and
paging what it writes, `%s` standing for the file name; stdin is read
as it is. The command is the rest of the line, so `open_cmd bat
--color=always --paging=never %s` works; a pipeline needs a script. A
command that fails ends the text with a read error naming its exit
status and stderr. `R` and auto-reload run it again. `--open-cmd`
overrides it for one run, and `--no-open-cmd` reads the files as they
are for one run; see [Pager](pager.md#bat).

`tab_width N` draws a tab as N cells: the next multiple of N from the
row's start. 8 is the default. `--tab-width` overrides it for one run.

`edge_markers yes` marks text off the side of the screen in nowrap
mode: a row whose line runs past the right edge ends in `>`, and one
with text scrolled off the left edge starts with `<`, both in reverse
video. In wrap mode a row whose line continues on the next ends in
`\`, and text wraps a column early to leave the last column to it.
`edge_markers no` is the default. `--edge-markers` and
`--no-edge-markers` override it for one run.

`file_separators no` leaves out the row that names each input when
several are given; see [Pager](pager.md#the-inputs-name).
`file_separators yes` is the default. `--file-separators` and
`--no-file-separators` override it for one run.

Flags in the `VEDI` environment variable override these for every
run, and the command line overrides `VEDI`; see
[Pager](pager.md#vedi).

## Commands

`:` opens a prompt on the status line. It takes the settings above
and `map`, as the config file does, for the rest of the run:
`:wrap_style word`, `:tab_width 4`, `:map x quit`. `:22`, or `:goto
22`, goes to line 22. `:cycle wrap_style` sets a setting to the value
after its current one, the first after the last; it takes `wrap`,
`wrap_style`, `edge_markers`, `file_separators` and `auto_reload`,
and reports the new value on the status line unless the line shows
it anyway, as it does `wrap` and `wrap_style`. A bad line is an error
on the status line.
`clear_all_shortcuts` is not taken: it would unbind `:` and `q`.

Tab completes the word being typed: the command, `yes` or `no`, `char`
or `word`, a setting after `cycle`, an action after `map`'s key. One
match fills it in and a space; several, what they share, and lists
them sorted after the prompt. Tab again fills in each in turn, then
what was typed. Enter on a listed command keeps the prompt, for the
argument; on a listed argument it runs the line. Up and Down recall
earlier commands; Esc, Ctrl+g and Ctrl+c cancel.

## Keys

`Up Down Left Right Home End PgUp PgDn Enter Esc Tab Backspace Space`, or
a single character, with any of `Shift+`, `Ctrl+`, `Alt+`, `Cmd+` in
front. `Ctrl+f` and `Ctrl+F` are the same key, and `Ctrl+Space` is a
key too. `Shift+` on a character
needs `Cmd+`: without it Shift is another character, and `Shift+Space`
is not a key a terminal can send. `Cmd+G` is `Cmd+Shift+g`. `Cmd+`
keys reach vedi only on macOS and only where the terminal passes them
on. Under the kitty keyboard protocol a shifted character with `Alt+`
or `Cmd+`, as `Alt+<`, is read as on a US layout. `h` shows the
bindings in effect.

## Actions

Each movement has a `select_` twin that extends the selection instead
of clearing it: `left` and `select_left`, `page_down` and
`select_page_down`. `set_mark` starts a selection that every motion
then extends, Shift or not, until `set_mark` again, `clear_selection`
or a click ends it: vim's `v`, emacs's `Ctrl+Space`. `cycle` takes a
setting, as `cycle wrap_style`, and is the one action that does; see
[Commands](#commands). The defaults below name every other action.

## Defaults

```
# vedi's default bindings: "map <key> <action>", later lines winning.
# Cmd+ lines are macOS only. See docs/configuration.md.

# Move the cursor
map Up up
map Down down
map Left left
map Right right
map Home home
map End end
map j down
map k up
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

# First line, last line
map g first
map G last
map < first
map > last
map Cmd+Up first
map Cmd+Down last

# Previous input, next input
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
map Cmd+Shift+Left select_home
map Cmd+Shift+Right select_end
map Cmd+Shift+Up select_first
map Cmd+Shift+Down select_last

# Extend by a page
map Shift+PgUp select_page_up
map Shift+PgDn select_page_down

# Extend by half a page: select_half_page_up, select_half_page_down;
# unbound by default

# Start a selection that motions extend; again, end it: set_mark, as
# v in vim or Ctrl+Space in emacs; unbound by default

# Extend by word
map Ctrl+Shift+Left select_word_left
map Ctrl+Shift+Right select_word_right
map Alt+Shift+Left select_word_left
map Alt+Shift+Right select_word_right

# Select all
map Ctrl+a select_all
map Cmd+a select_all

# Clear the selection, then the search highlight
map Esc clear_selection

# Copy the selection as plain text
map Ctrl+c copy
map Cmd+c copy
map y copy

# Copy the selection and quit; with none, down a line
map Enter copy_and_quit

# Search; ignores case unless capitalized; empty repeats
map / search

# Search backward
map ? search_back

# Next and previous match; ? swaps them
map n search_next
map N search_prev
map Cmd+g search_next
map Cmd+Shift+g search_prev

# Command prompt; N goes to line N
map : command

# Toggle wrap / nowrap
map w cycle wrap

# Reload the file
map R reload

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
# 0 and $ go to the line's ends
map 0 home
map $ end
# w and b move by word; wrap moves to Alt+w, and Ctrl+b still pages up
map w word_right
map Alt+w cycle wrap
map b word_left
# Shifted, the same keys extend the selection
map H select_left
map L select_right
map W select_word_right
map B select_word_left
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
