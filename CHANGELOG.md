# Changelog

## v2.5.3 - 2026-10-28

### Fixes

- write keeps a selected last newline

## v2.5.2 - 2026-10-28

### Fixes

- G goes to the end of the text, past a trailing newline

## v2.5.1 - 2026-10-28

### Fixes

- Ctrl+Home and Ctrl+End go to the first and last line

## v2.5.0 - 2026-10-25

### Features

- & lists the lines that match, to copy or to go to one

## v2.4.1 - 2026-10-23

### Features

- a man page, `man vedi`
- AUR packages, vedi and vedi-bin

## v2.4.0 - 2026-10-22

### Features

- the prompt has a cursor, and Alt+Backspace deletes a word

### Fixes

- a :write name starting with ~ is in the home directory
- G lands on the last row of a wrapped last line
- the row naming a file stands out

## v2.3.0 - 2026-10-21

### Features

- write the text, or the selection, to a file from the : prompt

### Fixes

- startup errors spell out controls in a file name
- a C1 control in a name or title cannot reach the terminal
- a file name starting with - cannot be an option to open_cmd
- tab_width and one_page_rows_below stop at a row's cells
- a clipboard command runs in the background
- a paste is text for the prompt, never keys

### Performance

- the cursor visibility walks stop at the cursor's row

## v2.2.1 - 2026-10-19

### Fixes

- drop C1 controls and non-SGR m sequences while paging

## v2.2.0 - 2026-10-17

### Features

- T, M and B put the cursor on the top, middle and bottom row
- 0 and $ go to the start and end of the line
- an unbound key says so on the status line

### Fixes

- the page keys and the wheel scroll the view and leave the cursor
- only a motion brings a view scrolled off the cursor back
- the completion list shows the match filled in

### Performance

- watch every file in one FSEvents stream rebuild

## v2.1.0 - 2026-10-16

### Features

- Tab fills in the first match when the matches share nothing more
- Tab after a setting offers the value after its current
- a long name in the status line keeps its first directory and its end

### Fixes

- a recalled command completes its last word on

## v2.0.0 - 2026-10-12

### Breaking changes

- vedi is written in rust: go install no longer builds vedi; use brew, a
  release archive or cargo install --git.

### Features

- O toggles reading through open_cmd

### Fixes

- a reload keeps the cursor and selection in their input

### Performance

- the search walk borrows a block's bytes when it can
- one frame grid is reused between draws
- a chunk read from stdin fills 64 KiB with its Arc counts
- the -F wait blocks instead of polling for a resize
- a drawn line borrows the block cache instead of cloning
- queued progress is taken up once

## v1.12.0 - 2026-10-08

### Features

- open_cmd picks the command by file name

## v1.11.0 - 2026-10-04

### Features

- status_line no hides the status line
- help, reload and quit are commands
- view_style plain draws the text without its colors
- view_style raw shows the escapes as text

## v1.10.0 - 2026-10-04

### Features

- one_page_rows_below sets the rows -F leaves below the text
- --completion prints a completion script for bash, zsh or fish
- the match the cursor is in is drawn in white on magenta

## v1.9.0 - 2026-10-04

### Features

- [ and ] stop at lines that set a window title

### Fixes

- -F prints only the text, its colors and links

### Performance

- search skips blocks whose bytes rule out a match

## v1.8.0 - 2026-10-03

### Features

- the status line names the input under the cursor
- open_cmd reads each input through a command
- the search keys search the help screen
- a row names each input, and lines are numbered within it
- [ and ] move between inputs

### Fixes

- open_cmd reads files only; what is piped in is read as it is
- --no-quit-if-one-page turns off -F
- --no-open-cmd and --no-clipboard-cmd turn the commands off
- --nowrap is --no-wrap, as the other --no- flags are spelled
- a file without a final newline ends its own last line

## v1.7.0 - 2026-10-02

### Features

- Shift+click extends the selection
- VEDI holds flags for every run
- wrap_style word wraps rows at words
- edge markers mark wrapped rows
- the : prompt takes commands
- :auto_reload turns reloading on or off for the run
- Up and Down recall earlier commands
- Tab completes at the : prompt
- Ctrl+g and Ctrl+c cancel the prompts
- cycle steps a setting to its next value

### Fixes

- a search match off the screen lands on the top row
- a view scrolled sideways holds as input arrives
- the status line says word wrap, not word
- Enter on a listed command adds the space after it
- auto_reload no drops the reload a change queued

## v1.6.2 - 2026-10-01

### Fixes

- a keycap takes two cells

### Performance

- move to tcell v3
- draw over the last frame instead of clearing

## v1.6.1 - 2026-09-30

### Fixes

- a match on a combining mark shows on its letter, with the cursor
- an emoji sequence is one character

### Performance

- benchmark layout and drawing of accented, CJK and emoji text
- hand tcell a cluster as one string

## v1.6.0 - 2026-09-29

### Features

- edge_markers marks text off either side in nowrap mode

### Fixes

- -F decides before the screen opens, so short text never flashes through it

## v1.5.1 - 2026-09-26

### Fixes

- the wheel scrolls sideways, and with Shift

## v1.5.0 - 2026-09-26

### Features

- ⌘ keys on macOS
- help lists the keys bound to each action
- configurable key bindings
- J and K extend the selection by a line
- help screen laid out like less's, that scrolls
- clear_all_shortcuts starts the map from nothing
- set_mark starts a selection that motions extend
- reload a file that changes on disk, and on R
- --no-auto-reload and auto_reload no leave a changed file to R
- wrap no and clipboard_cmd in the config, --wrap for one run
- tab_width N in the config, --tab-width for one run

### Fixes

- a reload leaves the bindings scrolled where they were
- clear_all_shortcuts alone leaves no bindings, not the defaults
- with the mark set, a row motion past the ends extends to them
- -F measures tabs at the terminal's stops, not tab_width
- the rename-replace tests age the seed file first
- nothing fires after the watch is stopped

## v1.4.0 - 2026-09-21

### Features

- triple-click selects the line

### Fixes

- extending past the text's ends from any row motion

### Performance

- index every 64th line

## v1.3.0 - 2026-09-20

### Features

- keep the input as bytes, decode lines on demand
- page files from disk

### Fixes

- index lines outside the buffer lock

## v1.2.0 - 2026-09-20

### Features

- repeat last search with empty query
- add search query history
- auto-scroll while dragging at an edge
- keep OSC 8 hyperlinks clickable

## v1.1.0 - 2026-09-20

### Features

- less keys j k f d u < > ^F ^B ^D ^U ⌥v, ? searches backward, h is help
- search matches are black on bright yellow
- -F quits and prints the text if it fits the screen

### Fixes

- Terminal.app support

## v1.0.0 - 2026-09-20

First release.
