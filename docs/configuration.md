# Configuration

- [The file](#the-file)
- [Settings](#settings)
- [Commands](#commands)
- [Keys](#keys)
- [Actions](#actions)
- [Defaults](#defaults)
- [vim](#vim)
- [emacs](#emacs)

## The file

vedi reads `~/.config/vedi/vedi.conf` (or `$XDG_CONFIG_HOME/vedi/vedi.conf`),
or the file named by `--config`. Without a file it uses the
[defaults](#defaults). A bad line stops vedi before it opens, naming
the file and line.

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

`map <key> <action>` binds a key. Later lines win over earlier ones
and over the defaults; the action `none` unbinds.
`clear_all_shortcuts` drops every binding so far, defaults included.

## Settings

| Setting | Default | Flags | Effect |
| --- | --- | --- | --- |
| `auto_reload` | `yes` | `--auto-reload`, `--no-auto-reload` | Reread a file that changes on disk. `R` reloads either way. |
| `wrap` | `yes` | `--wrap`, `-S` | Start in wrap mode. `w` toggles. |
| `wrap_style` | `char` | `--wrap-style` | `word` breaks rows at spaces and tabs. |
| `tab_width` | `8` | `--tab-width` | Cells per tab stop. |
| `edge_markers` | `no` | `--edge-markers`, `--no-edge-markers` | Mark text that runs off the screen. |
| `file_separators` | `yes` | `--file-separators`, `--no-file-separators` | Name each input in a row when several are given. |
| `clipboard_cmd` | | `--clipboard-cmd`, `--no-clipboard-cmd` | Pipe copied text to a command instead of OSC 52. |
| `open_cmd` | | `--open-cmd`, `--no-open-cmd` | Read each file through a command; `%s` is the file name. |

Flags apply for one run. Flags in the `VEDI` environment variable
override the file, and the command line overrides `VEDI`; see
[Pager](pager.md#vedi).

**wrap_style word.** A word that does not fit moves to the next row,
taking the space after it; one wider than the screen breaks at the
edge. The status line shows `word wrap`.

**edge_markers yes.** In nowrap mode a row that runs past the right
edge ends in `>`, and one scrolled off the left edge starts with
`<`, both in reverse video. In wrap mode a row that continues on the
next ends in `\`, and text wraps a column early to leave room for it.

**clipboard_cmd** and **open_cmd** take the rest of the line as the
command, so `clipboard_cmd xclip -selection clipboard` and `open_cmd
bat --color=always --paging=never %s` work; a pipeline needs a
script. `open_cmd` leaves stdin as it is. A command that fails ends
the text with a read error naming its exit status and stderr; `R` and
auto-reload run it again. See [Pager](pager.md#bat) and
[Pager](pager.md#the-inputs-name).

## Commands

`:` opens a prompt on the status line. It takes any setting above,
and `map`, for the rest of the run:

    :wrap_style word
    :tab_width 4
    :map x quit

`:22`, or `:goto 22`, goes to line 22. `:cycle wrap_style` steps a
setting to its next value, the first after the last; it takes `wrap`,
`wrap_style`, `edge_markers`, `file_separators` and `auto_reload`.
`clear_all_shortcuts` is refused: it would unbind `:` and `q`. A bad
line is an error on the status line.

Tab completes the word being typed. One match fills it in; several
fill in what they share and list the rest under the prompt, and Tab
again steps through them. Up and Down recall earlier commands; Esc,
Ctrl+g and Ctrl+c cancel.

## Keys

A key is one of `Up Down Left Right Home End PgUp PgDn Enter Esc Tab
Backspace Space`, or a single character, with any of `Shift+`,
`Ctrl+`, `Alt+`, `Cmd+` in front: `q`, `Ctrl+f`, `Alt+Shift+Left`,
`Ctrl+Space`.

- `Ctrl+f` and `Ctrl+F` are the same key.
- A shifted character is just that character: `G`, not `Shift+g`.
  Only `Cmd+` takes `Shift+` on a character, and `Cmd+G` is
  `Cmd+Shift+g`. `Shift+Space` is not a key a terminal can send.
- `Cmd+` keys reach vedi only on macOS, and only where the terminal
  passes them on.
- Under the kitty keyboard protocol a shifted character with `Alt+`
  or `Cmd+`, as `Alt+<`, is read as on a US layout.

`h` shows the bindings in effect.

## Actions

The [defaults](#defaults) below name every action, with three
exceptions:

- Each movement has a `select_` twin that extends the selection
  instead of clearing it: `left` and `select_left`, `page_down` and
  `select_page_down`.
- `set_mark` starts a selection that every motion then extends until
  `set_mark` again, `clear_selection` or a click ends it: vim's `v`,
  emacs's `Ctrl+Space`.
- `cycle <setting>` steps a setting, as in [Commands](#commands). It
  is the one action that takes an argument.

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
