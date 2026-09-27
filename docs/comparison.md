# vedi and other pagers

vedi has a cursor. The arrows move it through the text, Shift extends
a selection from it, and copy sends the selected text, plain, to the
clipboard. A selection can run past the screen, and a wrapped line
copies as one line. That is the reason vedi exists; for paging,
selecting and searching colored text. The pagers below do as well,
each in its own way and with strengths of its own.

vedi also stops there. It draws the colors it is given and adds none.
Syntax highlighting, diff formatting, column layout and the like
are not its job; tools such as bat, delta and column do them and pipe
the result through vedi.

## less

less leaves selection to the terminal, which takes the characters on
screen. The alternate screen has no scrollback, so a selection stays
within one screen, and in tmux with the mouse on, the drag goes to
tmux. To copy more, less has marks: `m` and a letter marks the top
line, and after a move, `|` and the letter pipes the lines from there
to the screen through a command, such as `pbcopy`. Nothing shows
which lines are marked until they are copied, and a mark is a line,
so the copy cannot start or end partway through one.

## moor

[moor](https://github.com/walles/moor) highlights source code
itself. It leaves selection to the terminal, and lets you choose
whether the mouse scrolls or selects, since a terminal sends the
wheel and the drag together.

## ov

[ov](https://github.com/noborus/ov) is built for tabular text: a
column mode colors the columns, and header lines and columns stay
fixed while the rest scrolls. It selects with the mouse, by click,
drag, double- and triple-click, and copies over OSC 52 or to a
command, as vedi does.

## tmux copy mode

tmux's copy mode has a cursor and keyboard selection, with vi or
emacs keys, over everything that has scrolled by in the pane. It
needs tmux, and it selects screen rows. A line the terminal wrapped
copies whole, but a line that a full-screen program drew across rows,
as most pagers do, copies with a newline at each row. vedi selects
the text's own lines, in any terminal.

## Speed and memory

`make bench-pagers` runs each pager on a pty over a million generated
lines, 54 columns of colored text each, and reports the median of
five runs.

| | vedi | less | moor | ov |
|---|---:|---:|---:|---:|
| first screen from a file, ms | 5.1 | 4.9 | 79.6 | 64.5 |
| then to the last line, ms | 100.3 | 233.0 | 105.7 | 2.3 |
| then back to the first, ms | 0.3 | 0.3 | 0.4 | 0.2 |
| a search that finds nothing, ms | 453.1 | 937.5 | 112.2 | 1712.8 |
| first screen from stdin, ms | 4.9 | 4.9 | 76.4 | 62.6 |
| that search, from stdin, ms | 441.5 | 912.8 | 112.2 | 1708.5 |
| peak memory, file, MB | 12.1 | 2.4 | 345.8 | 75.6 |
| peak memory, stdin, MB | 81.8 | 94.2 | 335.6 | 228.9 |

The search covers the whole input, which is read by then. moor
searches as each character is typed, so its time is four searches.

| Run | |
|---|---|
| Date | 2026-09-27 |
| Machine | Apple M3 Max, macOS 26.3 |
| vedi | 1.5.2 |
| less | 704 |
| moor | 2.19.2 |
| ov | 0.54.0 |
