# Keys

`h` shows the bindings in effect; the motions scroll them and the search keys
search them. These are the defaults.


| Key | Action |
|---|---|
| h | Show the key bindings |
| q | Quit |
| w | Toggle wrap / nowrap |
| W | Wrap at 80 cells / the screen's edge |
| R | Reload the file |
| O | Read through the next open_cmd |
| Up, Down, Left, Right, Home, End, j, k, 0, $, Enter, Cmd+Left, Cmd+Right | Move the cursor |
| PgDn, Space, f, Ctrl+f, Ctrl+v | Down a page |
| PgUp, b, Ctrl+b, Alt+v | Up a page |
| d, Ctrl+d | Down half a page |
| u, Ctrl+u | Up half a page |
| Ctrl+Left, Ctrl+Right, Alt+Left, Alt+Right, Alt+b, Alt+f | Move by word |
| g, G, <, >, Ctrl+Home, Ctrl+End, Cmd+Up, Cmd+Down | Start, end of the text |
| T, M, B | Top, middle, bottom of the shown text |
| s | Jump to a match or line by hint |
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
| / | Search as typed; ignores case if lowercase; empty repeats |
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

The Cmd+ keys reach vedi only where the terminal passes them on, over ssh too;
off macOS Cmd is Super. Some terminals keep or change other keys too;
[terminals.md](terminals.md) has the fixes.

## The cursor

The page keys and the wheel scroll the view and leave the cursor where it is,
off screen, so Shift with a click or an arrow selects from it across screens.
The next motion brings the view back to the cursor; T, M or B moves the cursor
onto the screen instead.

The status line names the cursor's line, or the top row's while the cursor is
off screen.

Text that ends in a newline has an empty row after its last line, where the
cursor can rest. It is not a line: the count stays what `wc -l` says, and the
status line shows the cursor there as one past the last, `line 5+1/5`. Rows
below the text start with a `~`; `end_markers no` in the config leaves them
blank.

A double-click selects a word, which spans the punctuation between it and the
next: a path or a link, `https://example.net/a`, is one word. Brackets and
quotes do not join, so `f(x)` is two.

## Search

`/` searches as the pattern is typed, each key going to the first match from
where the prompt opened; Esc puts the screen back as it was. The right of the
prompt row shows the match landed on and how many there are.

After Enter, the status line shows the pattern and the count, `/foo  3/17`, with
`?` for a backward search, until Esc clears the highlight. A match off the
screen scrolls to the top row, or as near as the last screenful allows.

## Hints

`s` puts a hint on each search match shown or, with no search highlighted, on
each line with text. Hints run 1 to 9, then letters, from the top, or from the
bottom with `hint_order bottom_up`.

Typing a hint moves the cursor there, so `n` goes on from that match. In the
filter's list, it goes to that place in the text, as Enter does. Any other key
takes the hints down.

## Filter

`&` lists only the lines that match the query as it is typed, colors kept. The
right of the prompt row counts them against the text's lines. Once there is a
query or the pointer moves, the pointed line is in reverse video.

- A search whose highlight is on starts the query, its matches listed at once;
  Ctrl+u clears it.
- Ctrl+n, Ctrl+p, PgDn and PgUp move the pointer, Up and Down recall earlier
  queries, and the wheel scrolls.

Enter keeps the list and gives it the keys: it scrolls, searches, selects and
copies as the text does, so `y` copies the matching lines and `:write` saves
them. Enter on the list goes to the pointed line in the text, in the middle of
the screen or as near as the text's ends allow, with the query as the search
pattern for `n` and `N`. `&` brings the list back as it was. Esc leaves the
list, after clearing a selection.

Lines still being read join the list as they come, and a reload makes it again.

## Drawing

The selection is drawn in reverse video. Search matches are black on bright
yellow, and the one the cursor is in bright white on magenta, whatever the
text's own colors.

A message follows the status line's summary until the next key: black on yellow
for what was done, `search wrapped`, and bright white on red for what could not
be, `not found: foo`, or a read error. On a line too narrow for both, the
message has it alone.

With `edge_markers yes` in the config, nowrap mode marks text off the screen
with a `<` or `>` in reverse video at that edge of the row, and wrap mode ends a
row that continues below with a `\`.

## Rebinding

Any key can be rebound in `~/.config/vedi/vedi.conf`, or the file named by
`--config`; `clear_all_shortcuts` there starts from no bindings. A key with no
binding says so on the status line.

Two actions are unbound by default:

- `set_mark` starts a selection that every motion extends, as `v` does in vim.
- `copy_and_quit` copies the selection and quits, or with none moves down a
  line.

See [configuration.md](configuration.md), which ends with vim and emacs maps.
