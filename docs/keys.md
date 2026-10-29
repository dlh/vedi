# Keys

`h` shows the bindings in effect; the motions scroll them when they do not fit,
and the search keys search them. These are the defaults.

| Key | Action |
|---|---|
| h | Show the key bindings |
| q | Quit |
| w | Toggle wrap / nowrap |
| R | Reload the file |
| O | Toggle reading through open_cmd |
| Up, Down, Left, Right, Home, End, j, k, 0, $, Cmd+Left, Cmd+Right | Move the cursor |
| PgDn, Space, f, Ctrl+f, Ctrl+v | Down a page |
| PgUp, b, Ctrl+b, Alt+v | Up a page |
| d, Ctrl+d | Down half a page |
| u, Ctrl+u | Up half a page |
| Ctrl+Left, Ctrl+Right, Alt+Left, Alt+Right, Alt+b, Alt+f | Move by word |
| g, G, <, >, Ctrl+Home, Ctrl+End, Cmd+Up, Cmd+Down | Start, end of the text |
| T, M, B | Top, middle, bottom row of the screen |
| [, ] | Previous, next input or title |
| : | Command prompt; N goes to line N |
| J, K, Shift+Up, Shift+Down, Shift+Left, Shift+Right, Shift+Home, Shift+End, Ctrl+Shift+Home, Ctrl+Shift+End, Cmd+Shift+Left, Cmd+Shift+Right, Cmd+Shift+Up, Cmd+Shift+Down | Extend the selection |
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
| & | Filter: list the lines that match; Enter goes to one |
| Up, Down | Recall earlier lines at this prompt |
| Ctrl+n, Ctrl+p, PgDn, PgUp | Move the pointer at the & prompt |
| Left, Right, Home, End, Cmd+Left, Cmd+Right | Move in the line |
| Ctrl+Left, Ctrl+Right, Alt+Left, Alt+Right, Alt+b, Alt+f | Move by word |
| Alt+Backspace, Ctrl+Backspace, Ctrl+w | Delete the word before the cursor |
| Cmd+Backspace, Ctrl+u | Delete to the start of the line |
| Tab | Complete; again, the next match |
| Esc, Ctrl+g, Ctrl+c | Cancel |

The Cmd+ keys are macOS only, and reach vedi where the terminal passes them on:
kitty does, Cmd+c when it has no selection of its own; Terminal.app keeps them.
Terminal.app also sends some Shift+ keys unshifted, macOS terminals compose
Option+ keys unless told to send Alt, and kitty keeps Shift+click;
[terminals.md](terminals.md) has the fixes.

The page keys and the wheel scroll the view and leave the cursor where it is,
hidden off screen, so Shift with a click or an arrow selects from it across
screens. The next motion brings the view back to it, and T, M or B puts it on
the screen instead. The status line names the cursor's line, or the top row's
while the cursor is off screen. Text that ends in a newline has an empty row
after its last line, where the cursor rests past the text; it is no line, so
the count stays what `wc -l` says and the status marks the cursor there as one
past it, `line 5+1/5`.

The selection is drawn in reverse video and search matches in black on bright
yellow, the match the cursor is in in bright white on magenta, whatever the
text's own colors. A match off the screen scrolls to the top row, or as near as
the last screenful allows. With `edge_markers yes` in the config, nowrap mode
marks text off the screen with a `<` or `>` in reverse video at that edge of the
row, and wrap mode ends a row that continues below with a `\`.

`&` filters. As the query is typed, the text rows list only the lines that
match, colors kept, the pointed line in reverse video, and the right of the
prompt row counts them against the text's lines. Ctrl+n, Ctrl+p, PgDn and PgUp
move the pointer, Up and Down recall earlier queries, and the wheel scrolls.
Enter keeps the list and gives it the keys:
it scrolls, searches, selects and copies as the text does, so `y` copies the
matching lines, and `:write` saves them. Enter there goes to the line in the
text, in the middle of the screen or as near as the text's ends allow, with the
query as the search pattern for `n` and `N`, and `&` brings the list back as it
was. Esc leaves the list, after clearing a selection. Lines
that are still being read join the list as they come, and a reload makes it
again.

Any key can be rebound in `~/.config/vedi/vedi.conf`, or the file named by
`--config`, and `clear_all_shortcuts` there starts the map from nothing. A key
with no binding says so on the status line. The `set_mark` action, unbound by
default, starts a selection that every motion extends, as `v` does in vim. See
[configuration.md](configuration.md), which ends with vim and emacs maps.
