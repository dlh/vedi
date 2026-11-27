# Keys

These are the defaults. `h` shows the bindings in effect, and
[Using vedi](usage.md) says what the keys do in more detail.

| Key | Action |
|---|---|
| h | Show the key bindings |
| q | Quit |
| w | Toggle wrap / nowrap |
| W | Wrap at 80 cells / the screen's edge |
| # | Toggle the line numbers |
| R | Reload the file |
| O | Read through the next open_cmd |

## Moving

| Key | Action |
|---|---|
| Up, Down, Left, Right, Home, End, j, k, 0, $, Enter, Cmd+Left, Cmd+Right | Move the cursor |
| PgDn, Space, f, Ctrl+f, Ctrl+v | Down a page |
| PgUp, b, Ctrl+b, Alt+v | Up a page |
| d, Ctrl+d | Down half a page |
| u, Ctrl+u | Up half a page |
| Ctrl+Left, Ctrl+Right, Alt+Left, Alt+Right, Alt+b, Alt+f | Move by word |
| Alt+Up, Alt+Down | Move by paragraph |
| g, G, <, >, Ctrl+Home, Ctrl+End, Cmd+Up, Cmd+Down | Start, end of the text |
| T, M, B | Top, middle, bottom of the shown text |
| s | Jump to a match or line by hint |
| [, ] | Previous, next input or title |
| : | Command prompt; N goes to line N |

## Selecting

| Key | Action |
|---|---|
| J, K, Shift+Up, Shift+Down, Shift+Left, Shift+Right, Shift+Home, Shift+End, Ctrl+Shift+Home, Ctrl+Shift+End, Cmd+Shift+Left, Cmd+Shift+Right, Cmd+Shift+Up, Cmd+Shift+Down | Extend the selection |
| Shift+PgUp, Shift+PgDn | Extend by a page |
| Ctrl+Shift+Left, Ctrl+Shift+Right, Alt+Shift+Left, Alt+Shift+Right | Extend by word |
| Alt+Shift+Up, Alt+Shift+Down | Extend by paragraph |
| Ctrl+a, Cmd+a | Select all |
| Esc | Clear the selection, then the search highlight |

## Mouse

| Key | Action |
|---|---|
| Click, drag | Move the cursor, select; with Shift, extend; held at an edge, scroll |
| Double-, triple-click | Select the word, the line |
| Wheel | Scroll; with Shift, sideways |

## Copying

| Key | Action |
|---|---|
| Ctrl+c, Cmd+c, y | Copy the selection as plain text |

## Searching

| Key | Action |
|---|---|
| / | Search as typed; ignores case if lowercase; empty repeats |
| ? | Search backward |
| n, N, Cmd+g, Cmd+Shift+g | Next and previous match; ? swaps them |
| & | Filter: list the lines that match; Enter goes to one |

## Prompts

These work at the `/`, `?`, `&` and `:` prompts.

| Key | Action |
|---|---|
| Up, Down | Recall earlier lines at this prompt |
| Ctrl+n, Ctrl+p, PgDn, PgUp | Move the pointer at the & prompt |
| Left, Right, Home, End, Cmd+Left, Cmd+Right | Move in the line |
| Ctrl+Left, Ctrl+Right, Alt+Left, Alt+Right, Alt+b, Alt+f | Move by word |
| Alt+Backspace, Ctrl+Backspace, Ctrl+w | Delete the word before the cursor |
| Cmd+Backspace, Ctrl+u | Delete to the start of the line |
| Tab | Complete; again, the next match |
| Esc, Ctrl+g, Ctrl+c | Cancel |

## Keys your terminal may keep

- **Cmd+** keys reach vedi only where the terminal passes them on. That works
  over ssh too. Off macOS, Cmd is Super.
- **Alt+** keys on macOS need the terminal's Option-as-Alt setting.
- **Shift+** arrows and some other modified keys need a fix in Terminal.app.

[Terminals](terminals.md) has the settings.

## Rebinding

Any key can be rebound in `vedi.conf`; see
[Configuration](configuration.md#the-file). It ends with
[vim](configuration.md#vim) and [emacs](configuration.md#emacs) maps.

A key with no binding says so on the status line.

Two actions are unbound by default:

| Action | Does |
|---|---|
| `set_mark` | Starts a selection that every motion extends, as `v` does in vim. |
| `copy_and_quit` | Copies the selection and quits. With no selection, moves down a line. |
