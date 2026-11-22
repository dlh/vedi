# The template bin/theme-previews fills in for each theme: @NAME@ is
# the theme, @TERMINAL@ the vhs theme with its background, quoted or as
# JSON. The text is drawn plain, so only the theme colors it.

Set Shell bash
Set FontSize 14
# Over 80 cells wide, so the guide shows in the 81st.
Set Width 860
Set Height 340
Set Padding 12
Set TypingSpeed 0
Set Theme @TERMINAL@

Hide
Type `cd docs/media && conf=$(mktemp) && printf 'include %s\nmap m set_mark\n' "$PWD/../../vedi/themes/@NAME@.conf" > "$conf" && export VEDI="--wrap-style word --wrap-width 80 --view-style plain --config $conf" && clear`
Enter
Type "vedi sample.txt"
Enter
# vedi takes a moment to start; a key typed sooner is lost.
Sleep 2s

# Matches, the current one, and a selection to it from a mark, which
# a search extends where it would clear a selection; copying it puts a
# note on the status line. Rows wrap at 80 cells to draw the guide.
Type "jjm"
Type "/dolor"
Enter
Sleep 300ms
Type "nny"
Sleep 500ms
Show
Sleep 200ms
Screenshot docs/media/themes/@NAME@.png
# vhs writes the screenshot on a later frame.
Sleep 500ms
