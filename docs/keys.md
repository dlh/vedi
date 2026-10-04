# Keys

`h` shows the bindings in effect; the motions scroll them when they
do not fit, and the search keys search them. These are the defaults.

| Key | Action |
|---|---|
| h | Show the key bindings |
| q | Quit |
| w | Toggle wrap / nowrap |
| R | Reload the file |
| Up, Down, Left, Right, Home, End, j, k, Cmd+Left, Cmd+Right | Move the cursor |
| PgDn, Space, f, Ctrl+f, Ctrl+v | Down a page |
| PgUp, b, Ctrl+b, Alt+v | Up a page |
| d, Ctrl+d | Down half a page |
| u, Ctrl+u | Up half a page |
| Ctrl+Left, Ctrl+Right, Alt+Left, Alt+Right, Alt+b, Alt+f | Move by word |
| g, G, <, >, Cmd+Up, Cmd+Down | First line, last line |
| [, ] | Previous, next input or title |
| : | Command prompt; N goes to line N |
| J, K, Shift+Up, Shift+Down, Shift+Left, Shift+Right, Shift+Home, Shift+End, Cmd+Shift+Left, Cmd+Shift+Right, Cmd+Shift+Up, Cmd+Shift+Down | Extend the selection |
| Shift+PgUp, Shift+PgDn | Extend by a page |
| Ctrl+Shift+Left, Ctrl+Shift+Right, Alt+Shift+Left, Alt+Shift+Right | Extend by word |
| Ctrl+a, Cmd+a | Select all |
| Esc | Clear the selection, then the search highlight |
| Click, drag | Move the cursor, select; with Shift, extend; held at an edge, scroll |
| Double-, triple-click | Select the word, the line |
| Wheel | Scroll; with Shift, sideways |
| Ctrl+c, Cmd+c, y | Copy the selection as plain text |
| Enter | Copy the selection and quit; with none, down a line |
| / | Search; ignores case if lowercase; empty repeats |
| ? | Search backward |
| n, N, Cmd+g, Cmd+Shift+g | Next and previous match; ? swaps them |
| Up, Down | Recall earlier searches, or commands |
| Tab | Complete; again, the next match |
| Esc, Ctrl+g, Ctrl+c | Cancel |

The Cmd+ keys are macOS only, and reach vedi where the terminal passes
them on: kitty does, Cmd+c when it has no selection of its own;
Terminal.app keeps them. Terminal.app also sends some Shift+ keys
unshifted, and kitty keeps Shift+click; [terminals.md](terminals.md)
has the fixes.

The selection is drawn in reverse video and search matches in black
on bright yellow, whatever the text's own colors. A match off the
screen scrolls to the top row, or as near as the last screenful
allows. With `edge_markers yes` in the config, nowrap mode marks text
off the screen with a `<` or `>` in reverse video at that edge of the
row, and wrap mode ends a row that continues below with a `\`.

Any key can be rebound in `~/.config/vedi/vedi.conf`, or the file
named by `--config`, and `clear_all_shortcuts` there starts the map
from nothing. The `set_mark` action, unbound by default, starts a
selection that every motion extends, as `v` does in vim. See
[configuration.md](configuration.md), which ends with vim and emacs
maps.
