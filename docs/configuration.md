# Configuration

- [The file](#the-file)
- [Settings](#settings)
- [clipboard_cmd](#clipboard_cmd)
- [open_cmd](#open_cmd)
- [Colors](#colors)
- [Keys](#keys)
- [Actions](#actions)
- [Defaults](#defaults)
- [vim](#vim)
- [emacs](#emacs)

## The file

vedi reads `~/.config/vedi/vedi.conf`, or `$XDG_CONFIG_HOME/vedi/vedi.conf`.
`--config FILE` names another. Without a file it uses the
[defaults](#defaults).

One directive per line. Blank lines and `#` comments are skipped. A bad line
stops vedi before it opens, naming the file and line.

| Directive | Does |
| --- | --- |
| `<setting> <value>` | Sets a [setting](#settings). |
| `map <key> <action>` | Binds a [key](#keys) to an [action](#actions). The action `none` unbinds it. |
| `clear_all_shortcuts` | Drops every binding so far, defaults included. |
| `color <element> <style>` | Sets a [color](#colors). |
| `include FILE` | Reads another file there, as if its lines were in this one. |

Later lines win over earlier ones and over the defaults.

In `include FILE`, a leading `~` is the home directory, and a relative name is
from this file's directory.

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

| Setting | Values | Default | Effect | Flag |
| --- | --- | --- | --- | --- |
| `auto_reload` | `no`, `yes` | `yes` | Reread a file that changes on disk. `R` reloads either way. | `--[no-]auto-reload` |
| `wrap` | `no`, `yes` | `yes` | Start in wrap mode. `w` toggles. | `-S`, `--[no-]wrap` |
| [`wrap_style`](#wrap_style) | `char`, `word` | `char` | Break rows at the screen's edge, or at spaces and tabs. | `--wrap-style` |
| [`wrap_width`](#wrap_width) | A number | `0` | Wrap rows at this many cells when the screen is wider; `0` wraps at its edge. | `--wrap-width` |
| `tab_width` | A number | `8` | Cells per tab stop. | `--tab-width` |
| [`edge_markers`](#edge_markers) | `no`, `yes` | `no` | Mark text that runs off the screen. | `--[no-]edge-markers` |
| `file_separators` | `no`, `yes` | `yes` | Name each input in a row when [several are given](usage.md#several-files). | `--[no-]file-separators` |
| `end_markers` | `no`, `yes` | `yes` | Start each row below the text with `~`. | `--[no-]end-markers` |
| [`status_line`](#status_line) | `no`, `yes` | `yes` | Draw the status line. | `--[no-]status-line` |
| [`view_style`](#view_style) | `color`, `plain`, `raw` | `color` | Draw the text in color, without its colors, or with its escapes as text. | `--view-style` |
| `hint_order` | `bottom_up`, `top_down` | `top_down` | Number `s`'s hints from the top or the bottom of the screen. | `--hint-order` |
| [`line_numbers`](#line_numbers) | `no`, `yes` | `no` | Number the lines. | `--[no-]line-numbers` |
| `line_numbers_side` | `left`, `right` | `left` | The edge of the screen the line numbers are at. | `--line-numbers-side` |
| `one_page_rows_below` | A number | `1` | Rows `-F` leaves below the text, for the shell's prompt. | `--one-page-rows-below` |
| [`copy_on_select`](#copy_on_select) | `no`, `yes` | `yes` | Copy what the mouse selects when the button comes up. | `--[no-]copy-on-select` |
| [`clipboard_cmd`](#clipboard_cmd) | A command |  | Pipe copied text to a command instead of OSC 52. | `--[no-]clipboard-cmd` |
| [`open_cmd`](#open_cmd) | A command |  | Read each file through a command. | `--[no-]open-cmd` |

Where a value is set, strongest first:

1. A flag on the command line, for one run.
2. A flag in the [`VEDI`](pager.md#vedi) environment variable.
3. The config file.

A `:` [command](commands.md#settings) changes most settings for the rest of the
run.

### wrap_style

- `char` breaks a row at the screen's edge.
- `word` moves a word that does not fit to the next row, with the space after
  it. A word wider than the screen breaks at the edge. The status line shows
  `word wrap`.

### wrap_width

With `wrap_width 80`:

- Rows wrap at 80 cells, with a `│` in the 81st.
- With `edge_markers yes`, the `\` takes the 80th.

`W` switches between `0` and `80`.

### edge_markers

With `edge_markers yes`:

| Mode | Marker |
| --- | --- |
| nowrap | `>` ends a row that runs past the right edge. `<` starts one scrolled off the left edge. |
| wrap | `\` ends a row that continues on the next. Text wraps a column early to leave room for it. |

### status_line

With `status_line no`:

- The text has the bottom row.
- The line comes back for a prompt, a message, help or a read error, and goes
  with the next key.
- A copy the mouse makes says nothing unless it fails, so the row stays under
  the next click.

### view_style

`plain` draws the text in the terminal's default colors:

- Its SGR colors and attributes are left out. Links stay.
- `-F` prints the text without its colors too.
- The status line shows `plain`.

`raw` draws the input's bytes:

- An escape sequence is drawn as its characters and colors nothing. The ESC is
  drawn as `^[`.
- Search finds the escapes' text, copy sends it, and `-F` prints it.
- No [title](usage.md#titles) names stdin, and `[` and `]` stop only at inputs.
- The status line shows `raw`.

Changing to or from `raw` keeps the cursor's line and clamps its column.

### line_numbers

With `line_numbers yes`:

- Each line's first row has its number. A wrapped line's other rows have none.
- Lines are counted within their input, as the status line counts.
- The column is as wide as the text's line count, and the text has the columns
  left.
- With `line_numbers_side right`, the numbers are at the screen's edge, or after
  the `wrap_width` bar when there is one.
- In the filter's list, a line has the number it has in the text.

Nothing is numbered in help, on a screen too narrow to leave the text a column,
or in what `-F` prints.

### copy_on_select

By default releasing the mouse button copies what it selected, as a terminal
does when no program has taken the mouse. With `copy_on_select no`, only the
copy keys copy.

## clipboard_cmd

`clipboard_cmd` pipes copied text to a command. The command is the rest of the
line; a pipeline needs a script.

    clipboard_cmd xclip -selection clipboard

- It runs in the background, and the status line says `copying` until it is
  done.
- `copy_and_quit` waits for it.
- A second copy before then ends it, as does quitting.

[Terminals](terminals.md#clipboard) says when one is needed.

## open_cmd

`open_cmd` reads each file through a command, to color or format it. The command
is the rest of the line; a pipeline needs a script.

    open_cmd bat --color=always --paging=never %s

- `%s` is the file name. Without `%s`, the name is the command's last argument.
- A name starting with `-` is passed as `./-name`, so the command does not take
  it for an option.
- Text on stdin is read as it is.
- A command that fails ends the text with a read error naming its exit status
  and stderr.
- `R` and auto-reload run the command again.

### By file type

A glob before the command limits it to files whose name matches, case ignored:

    open_cmd *.md mdcat --ansi
    open_cmd --no-fallback *.json jq -C . %s
    open_cmd bat --color=always --paging=never %s

- A glob is a word with `*`, `?` or `[`. `[!a-c]` excludes, and `[*]` is a
  literal star.
- A line without a glob is a fallback.
- A file is read through the last line whose glob matches it, else the last
  fallback.

### Cycling

`O` (`cycle open_cmd`) steps each file through every line that matches it, then
reads it as it is. With the lines above:

| File | `O` steps through |
| --- | --- |
| `a.md` | mdcat, bat, as it is |
| `a.json` | jq, as it is |

- `--no-fallback` before a glob keeps the fallback lines from the files it
  matches.
- The status line names the command for the file at the cursor, or `''` for
  none.
- The selection is cleared.

## Colors

`color <element> <style>` sets how vedi draws one of its own elements:

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
| `error` | `bright_white on red` | A message saying what could not be done. |
| `status` | `reverse` | The status line, and a completion Tab fills in. |
| `edge_marker` | `reverse` | `<`, `>` and `\` with `edge_markers yes`. |
| `control` | `reverse` | A control character, drawn as `^X`. |
| `guide` | `dim` | The `wrap_width` bar and the `~` below the text. |
| `separator` | `cyan` | The row naming each input. |
| `separator_name` | `bold` | The input's name on that row. |
| `line_number` | `dim` | The numbers of `line_numbers`. |

### Styles

A style is `[color] [on color] [attribute...]`: the foreground, the background,
then attributes.

| Part | Values |
| --- | --- |
| Color | `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, or one with `bright_` in front |
| | A number from `0` to `255` |
| | `#rrggbb` |
| | `default`, the terminal's own |
| Attribute | `bold`, `dim`, `italic`, `underline`, `reverse`, `strike` |

- The named colors and numbers are the terminal's palette, so they follow its
  theme. `#rrggbb` does not.
- A line replaces the element's whole style: `color match on blue` is the
  default foreground on blue, not bold.
- A `control` style is drawn over the text's own, which shows where it sets no
  color.

### Themes

A theme is a file of `color` lines. [Themes](themes.md) shows each one vedi
ships. To use one, include it:

    include themes/NAME.conf

| Installed by | Themes are in |
| --- | --- |
| Homebrew | `$(brew --prefix)/share/vedi/themes/` |
| The AUR packages | `/usr/share/vedi/themes/` |
| A release archive | `themes/` |

Include a theme from there, or copy it to `~/.config/vedi/themes/` to change it.

## Keys

A key is a name or a single character, with any modifiers in front: `q`,
`Ctrl+f`, `Alt+Shift+Left`, `Ctrl+Space`.

| Part | Values |
| --- | --- |
| Name | `Up` `Down` `Left` `Right` `Home` `End` `PgUp` `PgDn` `Enter` `Esc` `Tab` `Backspace` `Space` |
| Modifier | `Shift+` `Ctrl+` `Alt+` `Cmd+` |

- `Ctrl+f` and `Ctrl+F` are the same key.
- A shifted character is just that character: `G`, not `Shift+g`.
- Only `Cmd+` takes `Shift+` on a character, and `Cmd+G` is `Cmd+Shift+g`.
- `Shift+Space` is not a key a terminal can send.
- `Cmd+` is Super off macOS. It reaches vedi only where the terminal passes it
  on.
- Under the kitty keyboard protocol a shifted character with `Alt+` or `Cmd+`,
  as `Alt+<`, is the one the terminal reports for its layout. A terminal that
  reports none is read as on a US layout.
- Under the kitty keyboard protocol a key with `Ctrl+`, `Alt+` or `Cmd+` that
  types a character, as Option+v types `√` on macOS, runs its own binding, or
  the character's if it has none.

`h` shows the bindings in effect. A key that cycles a setting through given
values is on a row naming them.

## Actions

The [defaults](#defaults) below name every action but these:

- Each movement has a `select_` twin that extends the selection instead of
  clearing it: `left` and `select_left`, `page_down` and `select_page_down`.
- `set_mark` starts a selection that every motion then extends until `set_mark`
  again, `clear_selection` or a click ends it: vim's `v`, emacs's `Ctrl+Space`.
- `cycle <setting> [value...]` steps a setting, as
  [`:cycle`](commands.md#cycle) does. It is the one action that takes an
  argument.

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
map Alt+< first
map Alt+> last
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
# Ctrl+s and Ctrl+r search; Ctrl+s needs `stty -ixon` in most terminals
map Ctrl+s search
map Ctrl+r search_back
# Alt+w copies; Ctrl+g clears the selection, like Esc
map Alt+w copy
map Ctrl+g clear_selection
# Ctrl+Space sets the mark: motions extend the selection until Ctrl+g
map Ctrl+Space set_mark
```
