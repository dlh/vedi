# vedi and other tools

- [less](#less)
- [moor](#moor)
- [ov](#ov)
- [tmux copy mode](#tmux-copy-mode)
- [vim and other editors](#vim-and-other-editors)
- [Speed and memory](#speed-and-memory)

vedi has a cursor. The arrows move it through the text, Shift extends a
selection from it, and copy sends the selected text, plain, to the clipboard. A
selection can run past the screen, and a wrapped line copies as one line. That
is the reason vedi exists; for paging, selecting and searching colored text.
Each tool below does part of that, in its own way, and has strengths of its own.

vedi also stops there. It draws the colors it is given and adds none. Syntax
highlighting, diff formatting, column layout and the like are not its job; tools
such as bat, delta and column do them and pipe the result through vedi.

## less

less leaves selection to the terminal, which takes the characters on screen. The
alternate screen has no scrollback, so a selection stays within one screen, and
in tmux with the mouse on, the drag goes to tmux. To copy more, less has marks:
`m` and a letter marks the top line, and after a move, `|` and the letter pipes
the lines from there to the screen through a command, such as `pbcopy`. Nothing
shows which lines are marked until they are copied, and a mark is a line, so the
copy cannot start or end partway through one.

## moor

[moor](https://github.com/walles/moor) highlights source code itself. It leaves
selection to the terminal, and lets you choose whether the mouse scrolls or
selects, since a terminal sends the wheel and the drag together.

## ov

[ov](https://github.com/noborus/ov) is built for tabular text: a column mode
colors the columns, and header lines and columns stay fixed while the rest
scrolls. It selects with the mouse, by click, drag, double- and triple-click,
and copies over OSC 52 or to a command, as vedi does.

## tmux copy mode

tmux's copy mode has a cursor and keyboard selection, with vi or emacs keys,
over everything that has scrolled by in the pane. It needs tmux, and it selects
screen rows. A line the terminal wrapped copies whole, but a line that a
full-screen program drew across rows, as most pagers do, copies with a newline
at each row. vedi selects the text's own lines, in any terminal.

## vim and other editors

`vim -R`, or `view`, has most of what vedi has: a cursor, a selection that `v`
starts and every motion extends, and `"+y` to copy it to the clipboard, wrapped
lines whole. Two things are missing. vim shows escape sequences as text, so
`git log --color | vim -` reads `^[[33m` where a pager shows yellow. A plugin
can color them. AnsiEsc conceals them in vim with syntax rules, which vim
applies only to the lines it draws, after one pass over the file. neovim no
longer runs AnsiEsc, and its own plugins, baleia.nvim among them, mark up the
whole buffer before the first screen; a past run, below, has the time and memory
for that. And vim reads its input to the end before it draws, so a pipe shows
nothing until the command feeding it exits, and a command that never exits shows
nothing at all. The copy also needs a vim built with clipboard support, or a
neovim with a provider such as `pbcopy`, and over ssh a plugin for OSC 52, which
neovim 0.10 turns on by itself. emacs in view-mode is the same shape, with the
region and the kill ring, and its ansi-color library can color a buffer after
the fact.

## Speed and memory

`make bench-pagers` runs each pager on a pty over a million generated lines, 54
columns of colored text each, and reports the median of five runs.

| | vedi | less | moor | ov | vim |
|---|---:|---:|---:|---:|---:|
| first screen from a file, ms | 5.0 | 5.3 | 34.8 | 64.8 | 544.3 |
| then to the last line, ms | 79.3 | 238.2 | 107.8 | 2.6 | 9.1 |
| then back to the first, ms | 0.2 | 0.3 | 0.4 | 0.2 | 4.9 |
| a search that finds nothing, ms | 15.9 | 991.4 | 122.6 | 1769.7 | 23.4 |
| first screen from stdin, ms | 4.7 | 4.9 | 28.3 | 63.4 | 663.7 |
| that search, from stdin, ms | 5.5 | 970.1 | 115.6 | 1724.8 | 21.7 |
| peak memory, file, MB | 8.5 | 2.4 | 345.7 | 76.0 | 99.0 |
| peak memory, stdin, MB | 76.8 | 94.2 | 353.9 | 228.0 | 174.7 |

The search covers the whole input, which is read by then. moor searches as each
character is typed, so its time is four searches. vim runs with its plugin and
no other configuration.

neovim is no longer in the run. In its last, on 2026-10-03, 0.12.5 with
baleia.nvim of 2026-06-01 took 7.3 s to the first screen and 2.2 GB of memory.

| Run | |
|---|---|
| Date | 2026-10-12 |
| Machine | Apple M3 Max, macOS 26.3 |
| vedi | 2.0.0 |
| less | 704 |
| moor | 2.19.2 |
| ov | 0.54.0 |
| vim | 9.2, AnsiEsc 13i, powerman's fork of 2019-04-07 |
