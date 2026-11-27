# Using vedi

- [The cursor](#the-cursor)
- [Selecting and copying](#selecting-and-copying)
- [Search](#search)
- [Hints](#hints)
- [Filter](#filter)
- [Files and titles](#files-and-titles)
- [The status line](#the-status-line)

[Keys](keys.md) lists every key, [Commands](commands.md) what `:` takes, and
[Colors](configuration.md#colors) what vedi draws in colors of its own.

## The cursor

vedi has a cursor, as an editor does. The arrows move it, and a selection starts
from it.

### Scrolling

Scrolling does not move the cursor, so the cursor can be off screen. The wheel
scrolls, as do the page and half-page keys: PgDn, PgUp, Space, `d`, `u` and
their [other keys](keys.md#moving).

With the cursor off screen:

| To | Use |
|---|---|
| Go back to the cursor | Any key that moves it, such as an arrow |
| Bring the cursor to the screen | T, M or B: the top, middle or bottom row |
| Select from the cursor to a place on screen | Shift+click |

### The row after the last line

When the text ends in a newline, the cursor can go one row past the last line.
That row is empty and is not counted as a line. The status line shows the cursor
there as `line 5+1/5`.

Rows below the text start with `~`. `end_markers no` leaves them blank.

### Paragraphs

Alt+Down goes to the next blank line that follows a paragraph. Alt+Up goes to
the previous blank line that comes before one.

- A paragraph is a run of lines with text.
- A line of only spaces counts as blank.
- With no blank line left, Alt+Down goes to the end of the text and Alt+Up to
  the start.

### Words

A double-click selects a word.

- Punctuation joins the words on either side of it, so a path or a link such as
  `https://example.net/a` is one word.
- Brackets and quotes do not join: `f(x)` is two words.

## Selecting and copying

- Shift with a motion extends the selection. A motion without Shift clears it,
  as does Esc.
- The mouse selects by drag, a word by double-click and a line by triple-click.
  It copies when the button comes up, unless `copy_on_select no`.
- Ctrl+c, Cmd+c and `y` copy the selection.

Copy sends plain text, and a wrapped line copies as one line.

## Search

`/` searches forward and `?` backward, as the pattern is typed. A lowercase
pattern ignores case.

At the prompt:

- Each key goes to the first match from where the prompt opened.
- The right of the prompt row shows the match landed on and how many there are.
- Esc puts the screen back as it was.
- Enter with no pattern repeats the last search.

After Enter:

- `n` and `N` go to the next and previous match.
- The status line shows the pattern and the count, `/foo  3/17`, with `?` for a
  backward search.
- Esc clears the highlight.

Matches are highlighted over the text's own colors, and the one the cursor is in
has a color of its own.

## Hints

`s` puts a hint on each search match shown. With no search highlighted, it puts
one on each line with text.

- Hints run 1 to 9, then letters, from the top. `hint_order bottom_up` starts
  from the bottom.
- Type a hint's label to move the cursor to that match or line.
- In the filter's list, a hint goes to that place in the text, as Enter does.
- Any other key takes the hints down.

## Filter

`&` lists only the lines that match the query as it is typed, colors kept. It
has two stages: the prompt, then the list.

### At the prompt

| Key | Does |
|---|---|
| Ctrl+n, Ctrl+p, PgDn, PgUp | Move the pointer |
| Up, Down | Recall earlier queries |
| Wheel | Scroll |
| Ctrl+u | Clear the query |
| Enter | Keep the list |
| Esc | Cancel |

- The right of the prompt row counts the matching lines against the text's
  lines.
- Once there is a query or the pointer moves, the pointed line is highlighted.
- A search whose highlight is on starts the query, its matches listed at once.

### In the list

After Enter the list takes the keys as the text does: it scrolls, searches,
selects and copies. So `y` copies the matching lines, and `:write` saves them.

| Key | Does |
|---|---|
| Enter | Go to the pointed line in the text |
| Esc | Clear a selection, then leave the list |

After Enter, the query becomes the search pattern for `n` and `N`, and `&`
brings the list back as it was. Esc forgets the query.

Lines still being read join the list as they come, and a reload makes it again.

## Files and titles

The status line starts with the input's name: the file, or `<stdin>`.

### Several files

- The name is the file the cursor is in and its number among them, `b.txt 2/3`.
- Lines are numbered within each file, and `:N` goes to line N of the cursor's.
- A row above each file names it. `file_separators no`, or
  `--no-file-separators`, leaves those rows out.
- `[` and `]` go to the previous and next file.

### Titles

A program can set the terminal's window title as it writes, with the escape
sequence OSC 0 or 2: `ESC ] 2 ; title BEL`. vedi neither passes it to the
terminal nor prints it under `-F`. Instead:

- For text read from stdin, the status line shows the title in effect at the
  cursor's line instead of `<stdin>`. A file keeps its name.
- `[` and `]` stop at each line that sets a title.

## The status line

    f.txt  line 12/340  /foo  3/17  wrap  [copied 6 chars]            h help

| Part | Shows |
|---|---|
| `f.txt` | The input's name. |
| `line 12/340` | The cursor's line, and how many lines there are. |
| `/foo  3/17` | The search pattern and which match the cursor is in, while a search is highlighted. |
| `wrap` | The wrap mode: `wrap`, `word wrap` or `nowrap`. `plain` or `raw` follows under those view styles. |
| `[copied 6 chars]` | A message, until the next key: what was done, or what could not be done. |
| `h help` | The key that shows the key bindings. |

`status_line no` gives the text the bottom row; see
[Configuration](configuration.md#status_line).
