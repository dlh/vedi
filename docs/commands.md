# Commands

`:` opens a prompt on the status line. A command that fails shows an error
there.

| Command | Effect |
| --- | --- |
| `:22`, `:goto 22` | Go to line 22. |
| `:help`, `:reload`, `:quit` | What `h`, `R` and `q` do. |
| `:<setting> <value>` | [Change a setting](#settings) for the rest of the run. |
| `:cycle <setting> [value...]` | [Step a setting](#cycle) to its next value. |
| `:map <key> <action>` | Bind a key, as in the [config file](configuration.md#the-file). |
| `:color <element> <style>` | Set a [color](configuration.md#colors), as in the config file. |
| `:write FILE` | [Save](#write) the text or the selection to a new file. |
| `:write! FILE` | Save the text or the selection, replacing a file that exists. |

## Settings

`:<setting> <value>` takes any [setting](configuration.md#settings) but
`clipboard_cmd`, `open_cmd` and `one_page_rows_below`:

    :wrap_style word
    :tab_width 4

`:map` binds as the file does. `clear_all_shortcuts` is refused: it would unbind
`:` and `q`.

## cycle

`:cycle <setting>` steps a setting to its next value, wrapping to the first. It
takes these settings:

- `wrap`, `wrap_style`
- `edge_markers`, `end_markers`, `file_separators`
- `line_numbers`, `line_numbers_side`
- `status_line`, `view_style`, `hint_order`
- `auto_reload`, `copy_on_select`
- `open_cmd`

`:cycle <setting> <value>...` steps through the values given instead:

    :cycle tab_width 4 8

- It goes to the value after the current one, or to the first when the current
  one is not among them.
- It takes any setting the prompt takes, with two or more different values.

`cycle` is also an [action](configuration.md#actions), so a key can step a
setting: `w` is `map w cycle wrap`.

## write

| Command | Saves |
| --- | --- |
| `:write FILE` | The text, or the selection when there is one, as plain text, as copy sends it. |
| `:write --raw FILE` | The input itself, byte for byte; a selection's lines whole. |
| `:write --color FILE` | What the screen shows, with its colors and links as SGR and OSC 8. |

`:write` refuses a file that exists. `:write!` takes the same flags and replaces
it:

| Command | When FILE exists |
| --- | --- |
| `:write FILE` | An error; nothing is written. |
| `:write! FILE` | FILE is replaced, the file being viewed included. |

The file name is the rest of the line. A leading `~` is the home directory.

## Editing

| Key | Does |
| --- | --- |
| Tab | Complete the word: a command, a setting's value or `write`'s file name |
| Up, Down | Recall earlier commands |
| Esc, Ctrl+g, Ctrl+c | Cancel |

- Completing a setting's value starts after the current one.
- A paste goes in as one line, its newlines dropped, here and at the search
  prompt.

[Keys](keys.md#prompts) lists the rest of the prompt keys.
