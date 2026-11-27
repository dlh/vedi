# Recorded by `make media`, as demo.tape is, from the repository root:
# the text is one of vedi's own source files, colored by bat as the
# open_cmd. Regenerates filter.gif.

Output docs/media/filter.gif

Set Shell bash
Set FontSize 14
Set Width 760
Set Height 440
Set Padding 12
Set TypingSpeed 60ms

# As in demo.tape: the copy lands in a temp file that pbpaste reads.
# vhs cannot send Alt+Shift+Down, so a temp config binds its
# select_paragraph_down to P.
Hide
Type `clip=$(mktemp) && conf=$(mktemp) && echo 'map P select_paragraph_down' > "$conf" && export VEDI="--clipboard-cmd 'cat > $clip' --open-cmd 'bat --color=always -p %s' --config $conf" && pbpaste() { cat "$clip"; } && clear`
Enter
Show

Type "vedi vedi/src/app/status.rs"
Sleep 500ms
Enter
Sleep 1.5s

# &: each letter typed narrows the list to the lines that match.
Type "&"
Sleep 400ms
Type@200ms "error"
Sleep 1.2s

# Enter keeps the list and gives it the keys; 0 goes to the start of
# the line, j moves down to fn read_error, and Enter goes to it in the
# file.
Enter
Sleep 600ms
Type@200ms "0jjjjjjjj"
Sleep 600ms
Enter
Sleep 1.5s

# P extends the selection by paragraph from the fn line, and Ctrl+c
# copies it.
Sleep 600ms
Type "P"
Sleep 800ms
Ctrl+C
Sleep 1.2s

# Quit and paste.
Type "q"
Sleep 800ms
Type "pbpaste"
Sleep 400ms
Enter
Sleep 4s
