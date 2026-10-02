package app

import "github.com/gdamore/tcell/v3"

// prompt is a line being typed on the status row, and the history of
// the lines entered at it before.
type prompt struct {
	text    []rune
	history [][]rune // lines entered, oldest first
	pos     int      // the entry text shows; len(history) is the draft
	draft   []rune   // what was typed before Up recalled an entry
}

// open empties the prompt.
func (p *prompt) open() {
	p.text, p.pos = p.text[:0], len(p.history)
}

// cancels reports whether a key closes a prompt without running it:
// Esc, Ctrl+g and Ctrl+c.
func cancels(ev *tcell.EventKey) bool {
	switch ev.Key() {
	case tcell.KeyEscape, tcell.KeyCtrlG, tcell.KeyCtrlC:
		return true
	}
	return false
}

// edit handles the keys every prompt shares: Up and Down walk the
// history, Down past the newest entry restoring what was typed before
// Up; Backspace deletes; a character types. It reports whether the key
// was one of those.
func (p *prompt) edit(ev *tcell.EventKey) bool {
	switch ev.Key() {
	case tcell.KeyUp:
		if p.pos == 0 {
			return true
		}
		if p.pos == len(p.history) {
			p.draft = append(p.draft[:0], p.text...)
		}
		p.pos--
		p.text = append(p.text[:0], p.history[p.pos]...)
	case tcell.KeyDown:
		if p.pos == len(p.history) {
			return true
		}
		p.pos++
		if p.pos == len(p.history) {
			p.text = append(p.text[:0], p.draft...)
		} else {
			p.text = append(p.text[:0], p.history[p.pos]...)
		}
	case tcell.KeyBackspace:
		if len(p.text) > 0 {
			p.text = p.text[:len(p.text)-1]
		}
	case tcell.KeyRune:
		p.text = append(p.text, []rune(ev.Str())...)
	default:
		return false
	}
	return true
}

// remember adds the text to the history unless it repeats the newest
// entry.
func (p *prompt) remember() {
	if n := len(p.history); n > 0 && string(p.history[n-1]) == string(p.text) {
		return
	}
	p.history = append(p.history, append([]rune(nil), p.text...))
}
