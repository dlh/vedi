# Recorded by `make media`, as demo.tape is, from the repository root.
# Regenerates hint.gif.

Output docs/media/hint.gif

Set Shell bash
Set FontSize 14
Set Width 760
Set Height 440
Set Padding 12
Set TypingSpeed 60ms

# vhs cannot send Shift+End, so a temp config binds its select_end to E.
Hide
Type `cd docs/media && conf=$(mktemp) && echo 'map E select_end' > "$conf" && export VEDI="--wrap-style word --config $conf" && clear`
Enter
Show

Type "vedi sample.txt"
Sleep 500ms
Enter
# vedi takes a moment to start; a key typed sooner is lost.
Sleep 2s

# s puts a hint on each line shown; 4 goes to the second paragraph,
# and E selects to its end.
Type "s"
Sleep 1.5s
Type "4"
Sleep 1.2s
Type "E"
Sleep 4s

Type "q"
Sleep 1s
