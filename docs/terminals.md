# Terminals

What a terminal needs before every key and the clipboard work in vedi.

| Terminal | See |
|---|---|
| kitty | [Shift+click](#shiftclick), [Cmd keys](#cmd-keys), [Option as Alt](#option-as-alt), [scrollback](#kitty-scrollback) |
| Ghostty, iTerm2 | [Option as Alt](#option-as-alt) |
| Terminal.app | [Cmd keys](#cmd-keys), [Option as Alt](#option-as-alt), [its other keys](#macos-terminalapp) |
| tmux | [Clipboard](#clipboard) |

## Clipboard

Copy uses OSC 52, which works in kitty, xterm, tmux (`set-clipboard on`) and
over ssh. Where it does not, set a command in the config:

| System | Line in `vedi.conf` |
|---|---|
| macOS | `clipboard_cmd pbcopy` |
| Wayland | `clipboard_cmd wl-copy` |
| X11 | `clipboard_cmd xclip -selection clipboard` |

`--clipboard-cmd CMD` sets the command for one run. Terminal.app does not
support OSC 52, so vedi uses `pbcopy` there by itself.

## Shift+click

A terminal keeps Shift+click for its own selection while a program has the
mouse. vedi asks for it with XTSHIFTESCAPE (`\033[>1s`).

- xterm and Ghostty grant it unless configured not to.
- kitty does not. Unmap its shifted presses in `kitty.conf`:

      mouse_map shift+left press grabbed
      mouse_map shift+left click grabbed
      mouse_map shift+left doublepress grabbed
      mouse_map shift+left triplepress grabbed

  Each removes a kitty default.

## Cmd keys

- kitty passes Cmd+ keys on to vedi, Cmd+c only when it has no selection of its
  own.
- Terminal.app keeps them.

## Option as Alt

On macOS the Option key composes a character unless the terminal is told to send
Alt. Alt+v, Alt+b and Alt+f arrive as `√`, `∫` and `ƒ`, which nothing is bound
to. Each terminal has a setting:

| Terminal | Setting |
|---|---|
| kitty | `macos_option_as_alt yes` in `kitty.conf` |
| Ghostty | `macos-option-as-alt = true` |
| iTerm2 | Profiles → Keys → Left Option key: Esc+ |
| Terminal.app | Profiles → Keyboard → Use Option as Meta key |

Option then no longer types symbols in that terminal.

## macOS Terminal.app

Terminal.app has no mapping for the keys below, so the program sees them
unmodified or as other keys. To use them, add each under Settings → Profiles →
Keyboard, with the action Send Text:

| Key | Text |
|-----|------|
| ⇧↑ | `\033[1;2A` |
| ⇧↓ | `\033[1;2B` |
| ⇧Home | `\033[1;2H` |
| ⇧End | `\033[1;2F` |
| ⇧⇞ | `\033[5;2~` |
| ⇧⇟ | `\033[6;2~` |
| ⌃Home | `\033[1;5H` |
| ⌃End | `\033[1;5F` |
| ⌃⇧Home | `\033[1;6H` |
| ⌃⇧End | `\033[1;6F` |
| ⌥↑ | `\033[1;3A` |
| ⌥↓ | `\033[1;3B` |
| ⌥⇧↑ | `\033[1;4A` |
| ⌥⇧↓ | `\033[1;4B` |

## kitty scrollback

The [kitty](https://sw.kovidgoyal.net/kitty/) terminal can hand its scrollback
to a pager, in either of two ways.

**A binding.** This line in `kitty.conf` opens the scrollback in vedi, scrolled
to the rows you were looking at, with the cursor where it was:

    map <shortcut> launch --type overlay --stdin-source=@screen_scrollback --stdin-add-formatting vedi --hint-order bottom_up --scrolled-by @scrolled-by --cursor-row @cursor-y --cursor-col @cursor-x

**The `scrollback_pager`.** This gets one line per screen row. `-S` keeps them
so, but a wrapped line then copies with a newline at each wrap:

    scrollback_pager vedi --hint-order bottom_up -S +INPUT_LINE_NUMBER
