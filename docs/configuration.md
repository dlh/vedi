# Configuration

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

## Keys

`Up Down Left Right Home End PgUp PgDn Enter Esc Backspace Space`, or
a single character, with any of `Shift+`, `Ctrl+`, `Alt+`, `Cmd+` in
front. `Ctrl+f` and `Ctrl+F` are the same key. `Shift+` on a character
needs `Cmd+`: without it Shift is another character, and `Shift+Space`
is not a key a terminal can send. `Cmd+G` is `Cmd+Shift+g`. `Cmd+`
keys reach vedi only on macOS and only where the terminal passes them
on. Under the kitty keyboard protocol a shifted character with `Alt+`
or `Cmd+`, as `Alt+<`, is read as on a US layout. `h` shows the
bindings in effect.

## Actions

Each movement has a `select_` twin that extends the selection instead
of clearing it: `left` and `select_left`, `page_down` and
`select_page_down`. The defaults below name every action.

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

# Go to a line number
map : go_to_line

# Toggle wrap / nowrap
map w toggle_wrap

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
map Alt+w toggle_wrap
map b word_left
# Shifted, the same keys extend the selection
map H select_left
map L select_right
map W select_word_right
map B select_word_left
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
```
