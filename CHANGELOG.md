# Changelog

## v2.11.0 - 2026-11-27

### Features

- `line_numbers` numbers the lines, `#` toggles ([05d18b0](https://github.com/dlh/vedi/commit/05d18b0))

### Fixes

- `&` shows file separators in its list ([e7e6a58](https://github.com/dlh/vedi/commit/e7e6a58))
- a combining mark after a cut tab stays on the tab's row ([3d8c71a](https://github.com/dlh/vedi/commit/3d8c71a))

Full diff: [v2.10.2...v2.11.0](https://github.com/dlh/vedi/compare/v2.10.2...v2.11.0)

## v2.10.2 - 2026-11-23

### Fixes

- draw CSI cursor forward as spaces and repeat as its character ([0108b82](https://github.com/dlh/vedi/commit/0108b82))

Full diff: [v2.10.1...v2.10.2](https://github.com/dlh/vedi/compare/v2.10.1...v2.10.2)

## v2.10.1 - 2026-11-23

### Fixes

- `:write!` makes its temp file with the old file's mode ([fa8e621](https://github.com/dlh/vedi/commit/fa8e621))
- a reload refuses a file swapped for a FIFO or device ([9e18a25](https://github.com/dlh/vedi/commit/9e18a25))
- a signal that ends vedi kills its commands ([978d033](https://github.com/dlh/vedi/commit/978d033))
- a file truncated, then written, keeps the cursor's place ([4c9f4a9](https://github.com/dlh/vedi/commit/4c9f4a9))

Full diff: [v2.10.0...v2.10.1](https://github.com/dlh/vedi/compare/v2.10.0...v2.10.1)

## v2.10.0 - 2026-11-21

### Features

- a `~` starts each row below the text ([b8c2ee0](https://github.com/dlh/vedi/commit/b8c2ee0))
- `Alt+Up` and `Alt+Down` move by paragraph ([930ed84](https://github.com/dlh/vedi/commit/930ed84))
- `color` and `include` set a theme ([9c3c42d](https://github.com/dlh/vedi/commit/9c3c42d))

### Fixes

- a file separator stops at `wrap_width` ([20c43d6](https://github.com/dlh/vedi/commit/20c43d6))

Full diff: [v2.9.0...v2.10.0](https://github.com/dlh/vedi/compare/v2.9.0...v2.10.0)

## v2.9.0 - 2026-11-21

### Features

- `O` steps through each matching `open_cmd` ([d211771](https://github.com/dlh/vedi/commit/d211771))
- `open_cmd --no-fallback` keeps the fallback lines from a pattern's files ([7d76833](https://github.com/dlh/vedi/commit/7d76833))
- `hint_order bottom_up` numbers hints from the bottom ([1e15586](https://github.com/dlh/vedi/commit/1e15586))
- `Cmd+` keys are bound on every OS ([fee2603](https://github.com/dlh/vedi/commit/fee2603))

Full diff: [v2.8.0...v2.9.0](https://github.com/dlh/vedi/compare/v2.8.0...v2.9.0)

## v2.8.0 - 2026-11-17

### Features

- s labels the matches or lines shown to go to one ([d8210ca](https://github.com/dlh/vedi/commit/d8210ca))

### Fixes

- & highlights no line until a query is typed or the pointer moves ([b1d1c44](https://github.com/dlh/vedi/commit/b1d1c44))

Full diff: [v2.7.0...v2.8.0](https://github.com/dlh/vedi/compare/v2.7.0...v2.8.0)

## v2.7.0 - 2026-11-08

### Features

- cycle a setting through given values ([4f8cd0c](https://github.com/dlh/vedi/commit/4f8cd0c))
- wrap_width wraps rows at a width on a wider screen ([5d52861](https://github.com/dlh/vedi/commit/5d52861))
- write saves plain text, as copy sends it ([18abe2d](https://github.com/dlh/vedi/commit/18abe2d))
- Tab completes write's file name ([b220252](https://github.com/dlh/vedi/commit/b220252))

### Fixes

- a double-click's word spans joining punctuation ([b5646a4](https://github.com/dlh/vedi/commit/b5646a4))

Full diff: [v2.6.1...v2.7.0](https://github.com/dlh/vedi/compare/v2.6.1...v2.7.0)

## v2.6.1 - 2026-11-06

### Fixes

- a message follows the summary on the status line ([6d66e3b](https://github.com/dlh/vedi/commit/6d66e3b))

Full diff: [v2.6.0...v2.6.1](https://github.com/dlh/vedi/compare/v2.6.0...v2.6.1)

## v2.6.0 - 2026-11-02

### Features

- / and ? search as the line is typed ([59379d2](https://github.com/dlh/vedi/commit/59379d2))
- the status line counts the matches ([2d88647](https://github.com/dlh/vedi/commit/2d88647))
- & starts with the search pattern ([2efb33f](https://github.com/dlh/vedi/commit/2efb33f))
- the mouse copies what it selects ([53d07cc](https://github.com/dlh/vedi/commit/53d07cc))
- Enter moves down a line ([01904ee](https://github.com/dlh/vedi/commit/01904ee))

### Fixes

- write saves through a symlink of the user's own, refuses another's ([e66cd7c](https://github.com/dlh/vedi/commit/e66cd7c))
- M takes the middle of the rows the text fills ([a3e6d35](https://github.com/dlh/vedi/commit/a3e6d35))
- a copy within a line counts its chars ([0cdf852](https://github.com/dlh/vedi/commit/0cdf852))

Full diff: [v2.5.5...v2.6.0](https://github.com/dlh/vedi/compare/v2.5.5...v2.6.0)

## v2.5.5 - 2026-10-31

### Fixes

- a drag holds the cell under the pointer and hides the cursor ([e9d9ed7](https://github.com/dlh/vedi/commit/e9d9ed7))

Full diff: [v2.5.4...v2.5.5](https://github.com/dlh/vedi/compare/v2.5.4...v2.5.5)

## v2.5.4 - 2026-10-30

### Fixes

- write! sets the temp file's permissions on its handle ([a897e30](https://github.com/dlh/vedi/commit/a897e30))

Full diff: [v2.5.3...v2.5.4](https://github.com/dlh/vedi/compare/v2.5.3...v2.5.4)

## v2.5.3 - 2026-10-28

### Fixes

- write keeps a selected last newline ([8fb27af](https://github.com/dlh/vedi/commit/8fb27af))

Full diff: [v2.5.2...v2.5.3](https://github.com/dlh/vedi/compare/v2.5.2...v2.5.3)

## v2.5.2 - 2026-10-28

### Fixes

- G goes to the end of the text, past a trailing newline ([db539e9](https://github.com/dlh/vedi/commit/db539e9))

Full diff: [v2.5.1...v2.5.2](https://github.com/dlh/vedi/compare/v2.5.1...v2.5.2)

## v2.5.1 - 2026-10-28

### Fixes

- Ctrl+Home and Ctrl+End go to the first and last line ([2337392](https://github.com/dlh/vedi/commit/2337392))

Full diff: [v2.5.0...v2.5.1](https://github.com/dlh/vedi/compare/v2.5.0...v2.5.1)

## v2.5.0 - 2026-10-25

### Features

- & lists the lines that match, to copy or to go to one ([327d7a4](https://github.com/dlh/vedi/commit/327d7a4))

Full diff: [v2.4.1...v2.5.0](https://github.com/dlh/vedi/compare/v2.4.1...v2.5.0)

## v2.4.1 - 2026-10-23

### Features

- a man page, `man vedi` ([943a678](https://github.com/dlh/vedi/commit/943a678))
- AUR packages, vedi and vedi-bin ([6a50c69](https://github.com/dlh/vedi/commit/6a50c69))

Full diff: [v2.4.0...v2.4.1](https://github.com/dlh/vedi/compare/v2.4.0...v2.4.1)

## v2.4.0 - 2026-10-22

### Features

- the prompt has a cursor, and Alt+Backspace deletes a word ([6c113d3](https://github.com/dlh/vedi/commit/6c113d3))

### Fixes

- a :write name starting with ~ is in the home directory ([cd38a77](https://github.com/dlh/vedi/commit/cd38a77))
- G lands on the last row of a wrapped last line ([018d786](https://github.com/dlh/vedi/commit/018d786))
- the row naming a file stands out ([e1a848d](https://github.com/dlh/vedi/commit/e1a848d))

Full diff: [v2.3.0...v2.4.0](https://github.com/dlh/vedi/compare/v2.3.0...v2.4.0)

## v2.3.0 - 2026-10-21

### Features

- write the text, or the selection, to a file from the : prompt ([b30e809](https://github.com/dlh/vedi/commit/b30e809))

### Fixes

- startup errors spell out controls in a file name ([9282820](https://github.com/dlh/vedi/commit/9282820))
- a C1 control in a name or title cannot reach the terminal ([bce6ee9](https://github.com/dlh/vedi/commit/bce6ee9))
- a file name starting with - cannot be an option to open_cmd ([0529f38](https://github.com/dlh/vedi/commit/0529f38))
- tab_width and one_page_rows_below stop at a row's cells ([bc58303](https://github.com/dlh/vedi/commit/bc58303))
- a clipboard command runs in the background ([09968de](https://github.com/dlh/vedi/commit/09968de))
- a paste is text for the prompt, never keys ([cec754d](https://github.com/dlh/vedi/commit/cec754d))

### Performance

- the cursor visibility walks stop at the cursor's row ([3052e19](https://github.com/dlh/vedi/commit/3052e19))

Full diff: [v2.2.1...v2.3.0](https://github.com/dlh/vedi/compare/v2.2.1...v2.3.0)

## v2.2.1 - 2026-10-19

### Fixes

- drop C1 controls and non-SGR m sequences while paging ([97bb89d](https://github.com/dlh/vedi/commit/97bb89d))

Full diff: [v2.2.0...v2.2.1](https://github.com/dlh/vedi/compare/v2.2.0...v2.2.1)

## v2.2.0 - 2026-10-17

### Features

- T, M and B put the cursor on the top, middle and bottom row ([6c7043e](https://github.com/dlh/vedi/commit/6c7043e))
- 0 and $ go to the start and end of the line ([595e3a2](https://github.com/dlh/vedi/commit/595e3a2))
- an unbound key says so on the status line ([2193cb5](https://github.com/dlh/vedi/commit/2193cb5))

### Fixes

- the page keys and the wheel scroll the view and leave the cursor ([7b24ce3](https://github.com/dlh/vedi/commit/7b24ce3))
- only a motion brings a view scrolled off the cursor back ([7615b7c](https://github.com/dlh/vedi/commit/7615b7c))
- the completion list shows the match filled in ([9d1f6a9](https://github.com/dlh/vedi/commit/9d1f6a9))

### Performance

- watch every file in one FSEvents stream rebuild ([2b74434](https://github.com/dlh/vedi/commit/2b74434))

Full diff: [v2.1.0...v2.2.0](https://github.com/dlh/vedi/compare/v2.1.0...v2.2.0)

## v2.1.0 - 2026-10-16

### Features

- Tab fills in the first match when the matches share nothing more ([f63fac9](https://github.com/dlh/vedi/commit/f63fac9))
- Tab after a setting offers the value after its current ([183f5a5](https://github.com/dlh/vedi/commit/183f5a5))
- a long name in the status line keeps its first directory and its end ([1d6c0f1](https://github.com/dlh/vedi/commit/1d6c0f1))

### Fixes

- a recalled command completes its last word on ([659b31a](https://github.com/dlh/vedi/commit/659b31a))

Full diff: [v2.0.0...v2.1.0](https://github.com/dlh/vedi/compare/v2.0.0...v2.1.0)

## v2.0.0 - 2026-10-12

### Breaking changes

- vedi is written in rust: go install no longer builds vedi; use brew, a release archive or cargo install --git ([1be531b](https://github.com/dlh/vedi/commit/1be531b))

### Features

- O toggles reading through open_cmd ([b0db6d9](https://github.com/dlh/vedi/commit/b0db6d9))

### Fixes

- a reload keeps the cursor and selection in their input ([46f7854](https://github.com/dlh/vedi/commit/46f7854))

### Performance

- the search walk borrows a block's bytes when it can ([e585717](https://github.com/dlh/vedi/commit/e585717))
- one frame grid is reused between draws ([1288962](https://github.com/dlh/vedi/commit/1288962))
- a chunk read from stdin fills 64 KiB with its Arc counts ([d0e3f53](https://github.com/dlh/vedi/commit/d0e3f53))
- the -F wait blocks instead of polling for a resize ([ee58ee8](https://github.com/dlh/vedi/commit/ee58ee8))
- a drawn line borrows the block cache instead of cloning ([6df116f](https://github.com/dlh/vedi/commit/6df116f))
- queued progress is taken up once ([d644742](https://github.com/dlh/vedi/commit/d644742))

Full diff: [v1.12.0...v2.0.0](https://github.com/dlh/vedi/compare/v1.12.0...v2.0.0)

## v1.12.0 - 2026-10-08

### Features

- open_cmd picks the command by file name ([866058f](https://github.com/dlh/vedi/commit/866058f))

Full diff: [v1.11.0...v1.12.0](https://github.com/dlh/vedi/compare/v1.11.0...v1.12.0)

## v1.11.0 - 2026-10-04

### Features

- status_line no hides the status line ([3ea45ed](https://github.com/dlh/vedi/commit/3ea45ed))
- help, reload and quit are commands ([907f39f](https://github.com/dlh/vedi/commit/907f39f))
- view_style plain draws the text without its colors ([1b395be](https://github.com/dlh/vedi/commit/1b395be))
- view_style raw shows the escapes as text ([b82c99f](https://github.com/dlh/vedi/commit/b82c99f))

Full diff: [v1.10.0...v1.11.0](https://github.com/dlh/vedi/compare/v1.10.0...v1.11.0)

## v1.10.0 - 2026-10-04

### Features

- one_page_rows_below sets the rows -F leaves below the text ([1b7ce2f](https://github.com/dlh/vedi/commit/1b7ce2f))
- --completion prints a completion script for bash, zsh or fish ([b932057](https://github.com/dlh/vedi/commit/b932057))
- the match the cursor is in is drawn in white on magenta ([cf1397e](https://github.com/dlh/vedi/commit/cf1397e))

Full diff: [v1.9.0...v1.10.0](https://github.com/dlh/vedi/compare/v1.9.0...v1.10.0)

## v1.9.0 - 2026-10-04

### Features

- [ and ] stop at lines that set a window title ([a3c08fb](https://github.com/dlh/vedi/commit/a3c08fb))

### Fixes

- -F prints only the text, its colors and links ([2599e12](https://github.com/dlh/vedi/commit/2599e12))

### Performance

- search skips blocks whose bytes rule out a match ([d730b50](https://github.com/dlh/vedi/commit/d730b50))

Full diff: [v1.8.0...v1.9.0](https://github.com/dlh/vedi/compare/v1.8.0...v1.9.0)

## v1.8.0 - 2026-10-03

### Features

- the status line names the input under the cursor ([6d78fde](https://github.com/dlh/vedi/commit/6d78fde))
- open_cmd reads each input through a command ([6945e85](https://github.com/dlh/vedi/commit/6945e85))
- the search keys search the help screen ([c975778](https://github.com/dlh/vedi/commit/c975778))
- a row names each input, and lines are numbered within it ([d129c93](https://github.com/dlh/vedi/commit/d129c93))
- [ and ] move between inputs ([e095dca](https://github.com/dlh/vedi/commit/e095dca))

### Fixes

- open_cmd reads files only; what is piped in is read as it is ([17298f0](https://github.com/dlh/vedi/commit/17298f0))
- --no-quit-if-one-page turns off -F ([40fa0d8](https://github.com/dlh/vedi/commit/40fa0d8))
- --no-open-cmd and --no-clipboard-cmd turn the commands off ([d00798a](https://github.com/dlh/vedi/commit/d00798a))
- --nowrap is --no-wrap, as the other --no- flags are spelled ([5e9b6b8](https://github.com/dlh/vedi/commit/5e9b6b8))
- a file without a final newline ends its own last line ([5fff60c](https://github.com/dlh/vedi/commit/5fff60c))

Full diff: [v1.7.0...v1.8.0](https://github.com/dlh/vedi/compare/v1.7.0...v1.8.0)

## v1.7.0 - 2026-10-02

### Features

- Shift+click extends the selection ([3bd0684](https://github.com/dlh/vedi/commit/3bd0684))
- VEDI holds flags for every run ([4795f47](https://github.com/dlh/vedi/commit/4795f47))
- wrap_style word wraps rows at words ([272ce82](https://github.com/dlh/vedi/commit/272ce82))
- edge markers mark wrapped rows ([c9b517c](https://github.com/dlh/vedi/commit/c9b517c))
- the : prompt takes commands ([fd63dae](https://github.com/dlh/vedi/commit/fd63dae))
- :auto_reload turns reloading on or off for the run ([5ce13a9](https://github.com/dlh/vedi/commit/5ce13a9))
- Up and Down recall earlier commands ([88b3f0c](https://github.com/dlh/vedi/commit/88b3f0c))
- Tab completes at the : prompt ([e05c613](https://github.com/dlh/vedi/commit/e05c613))
- Ctrl+g and Ctrl+c cancel the prompts ([2ff05ac](https://github.com/dlh/vedi/commit/2ff05ac))
- cycle steps a setting to its next value ([2aa7513](https://github.com/dlh/vedi/commit/2aa7513))

### Fixes

- a search match off the screen lands on the top row ([9e05997](https://github.com/dlh/vedi/commit/9e05997))
- a view scrolled sideways holds as input arrives ([26e9d85](https://github.com/dlh/vedi/commit/26e9d85))
- the status line says word wrap, not word ([fc783dd](https://github.com/dlh/vedi/commit/fc783dd))
- Enter on a listed command adds the space after it ([6921483](https://github.com/dlh/vedi/commit/6921483))
- auto_reload no drops the reload a change queued ([9644a40](https://github.com/dlh/vedi/commit/9644a40))

Full diff: [v1.6.2...v1.7.0](https://github.com/dlh/vedi/compare/v1.6.2...v1.7.0)

## v1.6.2 - 2026-10-01

### Fixes

- a keycap takes two cells ([547d51e](https://github.com/dlh/vedi/commit/547d51e))

### Performance

- move to tcell v3 ([68cd3df](https://github.com/dlh/vedi/commit/68cd3df))
- draw over the last frame instead of clearing ([9584844](https://github.com/dlh/vedi/commit/9584844))

Full diff: [v1.6.1...v1.6.2](https://github.com/dlh/vedi/compare/v1.6.1...v1.6.2)

## v1.6.1 - 2026-09-30

### Fixes

- a match on a combining mark shows on its letter, with the cursor ([32a3d71](https://github.com/dlh/vedi/commit/32a3d71))
- an emoji sequence is one character ([72b51c0](https://github.com/dlh/vedi/commit/72b51c0))

### Performance

- benchmark layout and drawing of accented, CJK and emoji text ([9cd3b6b](https://github.com/dlh/vedi/commit/9cd3b6b))
- hand tcell a cluster as one string ([0af8953](https://github.com/dlh/vedi/commit/0af8953))

Full diff: [v1.6.0...v1.6.1](https://github.com/dlh/vedi/compare/v1.6.0...v1.6.1)

## v1.6.0 - 2026-09-29

### Features

- edge_markers marks text off either side in nowrap mode ([eb02c01](https://github.com/dlh/vedi/commit/eb02c01))

### Fixes

- -F decides before the screen opens, so short text never flashes through it ([e845101](https://github.com/dlh/vedi/commit/e845101))

Full diff: [v1.5.1...v1.6.0](https://github.com/dlh/vedi/compare/v1.5.1...v1.6.0)

## v1.5.1 - 2026-09-26

### Fixes

- the wheel scrolls sideways, and with Shift ([f1fbb9e](https://github.com/dlh/vedi/commit/f1fbb9e))

Full diff: [v1.5.0...v1.5.1](https://github.com/dlh/vedi/compare/v1.5.0...v1.5.1)

## v1.5.0 - 2026-09-26

### Features

- ⌘ keys on macOS ([dcf7782](https://github.com/dlh/vedi/commit/dcf7782))
- help lists the keys bound to each action ([f3db5a6](https://github.com/dlh/vedi/commit/f3db5a6))
- configurable key bindings ([552baf6](https://github.com/dlh/vedi/commit/552baf6))
- J and K extend the selection by a line ([b2935e3](https://github.com/dlh/vedi/commit/b2935e3))
- help screen laid out like less's, that scrolls ([dd1b78c](https://github.com/dlh/vedi/commit/dd1b78c))
- clear_all_shortcuts starts the map from nothing ([5e9e4aa](https://github.com/dlh/vedi/commit/5e9e4aa))
- set_mark starts a selection that motions extend ([a3190a0](https://github.com/dlh/vedi/commit/a3190a0))
- reload a file that changes on disk, and on R ([7cb7cc9](https://github.com/dlh/vedi/commit/7cb7cc9))
- --no-auto-reload and auto_reload no leave a changed file to R ([65fa34f](https://github.com/dlh/vedi/commit/65fa34f))
- wrap no and clipboard_cmd in the config, --wrap for one run ([2171734](https://github.com/dlh/vedi/commit/2171734))
- tab_width N in the config, --tab-width for one run ([69bc7f7](https://github.com/dlh/vedi/commit/69bc7f7))

### Fixes

- a reload leaves the bindings scrolled where they were ([0cd13b7](https://github.com/dlh/vedi/commit/0cd13b7))
- clear_all_shortcuts alone leaves no bindings, not the defaults ([ae3396a](https://github.com/dlh/vedi/commit/ae3396a))
- with the mark set, a row motion past the ends extends to them ([2471627](https://github.com/dlh/vedi/commit/2471627))
- -F measures tabs at the terminal's stops, not tab_width ([ba7758a](https://github.com/dlh/vedi/commit/ba7758a))
- the rename-replace tests age the seed file first ([2a6960e](https://github.com/dlh/vedi/commit/2a6960e))
- nothing fires after the watch is stopped ([0dbf196](https://github.com/dlh/vedi/commit/0dbf196))

Full diff: [v1.4.0...v1.5.0](https://github.com/dlh/vedi/compare/v1.4.0...v1.5.0)

## v1.4.0 - 2026-09-21

### Features

- triple-click selects the line ([bbe9465](https://github.com/dlh/vedi/commit/bbe9465))

### Fixes

- extending past the text's ends from any row motion ([c4ea598](https://github.com/dlh/vedi/commit/c4ea598))

### Performance

- index every 64th line ([0019879](https://github.com/dlh/vedi/commit/0019879))

Full diff: [v1.3.0...v1.4.0](https://github.com/dlh/vedi/compare/v1.3.0...v1.4.0)

## v1.3.0 - 2026-09-20

### Features

- keep the input as bytes, decode lines on demand ([389964c](https://github.com/dlh/vedi/commit/389964c))
- page files from disk ([3baa36c](https://github.com/dlh/vedi/commit/3baa36c))

### Fixes

- index lines outside the buffer lock ([2c322de](https://github.com/dlh/vedi/commit/2c322de))

Full diff: [v1.2.0...v1.3.0](https://github.com/dlh/vedi/compare/v1.2.0...v1.3.0)

## v1.2.0 - 2026-09-20

### Features

- repeat last search with empty query ([4d24ede](https://github.com/dlh/vedi/commit/4d24ede))
- add search query history ([57c8508](https://github.com/dlh/vedi/commit/57c8508))
- auto-scroll while dragging at an edge ([5bba632](https://github.com/dlh/vedi/commit/5bba632))
- keep OSC 8 hyperlinks clickable ([0e6c3cf](https://github.com/dlh/vedi/commit/0e6c3cf))

Full diff: [v1.1.0...v1.2.0](https://github.com/dlh/vedi/compare/v1.1.0...v1.2.0)

## v1.1.0 - 2026-09-20

### Features

- less keys j k f d u < > ^F ^B ^D ^U ⌥v, ? searches backward, h is help ([bc01834](https://github.com/dlh/vedi/commit/bc01834))
- search matches are black on bright yellow ([f845432](https://github.com/dlh/vedi/commit/f845432))
- -F quits and prints the text if it fits the screen ([30f1238](https://github.com/dlh/vedi/commit/30f1238))

### Fixes

- Terminal.app support ([e88ef47](https://github.com/dlh/vedi/commit/e88ef47))

Full diff: [v1.0.0...v1.1.0](https://github.com/dlh/vedi/compare/v1.0.0...v1.1.0)

## v1.0.0 - 2026-09-20

First release.
