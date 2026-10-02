# Recorded by `make media`, which builds ./vedi and runs vhs from the
# repository root. Regenerates demo.gif.

Output docs/media/demo.gif

Set Shell bash
Set FontSize 14
Set Width 760
Set Height 440
Set Padding 12
Set TypingSpeed 60ms

# OSC 52 has nowhere to land here, so VEDI sends the copy to a temp
# file, and a pbpaste function reads it back. VEDI also wraps rows at
# words.
Hide
Type `export PATH=$PWD:$PATH && cd docs/media && clip=$(mktemp) && export VEDI="--wrap-style word --clipboard-cmd 'cat > $clip'" && pbpaste() { cat "$clip"; } && clear`
Enter
Show

Type "vedi sample.txt"
Sleep 500ms
Enter
Sleep 1.5s

# Search: matches light up, and the cursor lands on the first.
Type "/"
Sleep 300ms
Type "velit"
Sleep 500ms
Enter
Sleep 1s
Sleep 1s

# The match sits mid-line; J extends the selection down the wrapped rows
# to the end of the line.
Type@250ms "JJJ"
Sleep 800ms

# Ctrl+c copies; the status line says so. Then quit and paste.
Ctrl+C
Sleep 800ms
Sleep 1s
Type "q"
Sleep 800ms
Type "pbpaste"
Sleep 400ms
Enter
Sleep 3s
