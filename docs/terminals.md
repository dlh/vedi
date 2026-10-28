# Terminals

## Clipboard

Copy uses OSC 52, which works in kitty, xterm, tmux (`set-clipboard on`) and
over ssh. Where it does not, set `--clipboard-cmd pbcopy` (macOS),
`--clipboard-cmd wl-copy` (Wayland) or
`--clipboard-cmd 'xclip -selection clipboard'` (X11), or
`clipboard_cmd xclip -selection clipboard` in the config.

## Shift+click

A terminal keeps Shift+click for its own selection while a program has the
mouse. vedi asks for it with XTSHIFTESCAPE (`\033[>1s`), which xterm and Ghostty
grant unless configured not to. kitty does not; unmap its shifted presses in
`kitty.conf`:

    mouse_map shift+left press grabbed
    mouse_map shift+left click grabbed
    mouse_map shift+left doublepress grabbed
    mouse_map shift+left triplepress grabbed

Each removes a kitty default.

## Option as Alt

On macOS the Option key composes a character unless the terminal is told to send
Alt: Alt+v, Alt+b and Alt+f arrive as `√`, `∫` and `ƒ`, which nothing is bound
to. Each terminal has a setting:

| Terminal | Setting |
|---|---|
| kitty | `macos_option_as_alt yes` in `kitty.conf` |
| Ghostty | `macos-option-as-alt = true` |
| iTerm2 | Profiles → Keys → Left Option key: Esc+ |
| Terminal.app | Profiles → Keyboard → Use Option as Meta key |

Option then no longer types symbols in that terminal.

## macOS Terminal.app

Terminal.app has no mapping for ⇧↑, ⇧↓, ⇧Home, ⇧End, ⇧⇞, ⇧⇟, ⌃Home, ⌃End,
⌃⇧Home or ⌃⇧End, so the program sees them unmodified. To use them, add these
under Settings → Profiles → Keyboard, action Send Text:

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

It does not support OSC 52, so copy uses `pbcopy`.

## kitty

The [kitty](https://sw.kovidgoyal.net/kitty/) terminal can hand its scrollback
to a pager. This binding in `kitty.conf` opens it in vedi, scrolled to the rows
you were looking at, with the cursor where it was:

    map <shortcut> launch --type overlay --stdin-source=@screen_scrollback --stdin-add-formatting vedi --scrolled-by @scrolled-by --cursor-row @cursor-y --cursor-col @cursor-x

Or set vedi as the `scrollback_pager`. That gets one line per screen row — `-S`
keeps them so, but a wrapped line then copies with a newline at each wrap:

    scrollback_pager vedi -S +INPUT_LINE_NUMBER
