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
Hide
Type `clip=$(mktemp) && export VEDI="--clipboard-cmd 'cat > $clip' --open-cmd 'bat --color=always -p %s'" && pbpaste() { cat "$clip"; } && clear`
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
Type@200ms "0jjjjjj"
Sleep 600ms
Enter
Sleep 1.5s

# J extends the selection down over the function, and Ctrl+c copies it.
Type@200ms "JJJJJJJ"
Sleep 600ms
Ctrl+C
Sleep 1.2s

# Quit and paste.
Type "q"
Sleep 800ms
Type "pbpaste"
Sleep 400ms
Enter
Sleep 4s
