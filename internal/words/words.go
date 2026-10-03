// Package words splits a string into words as a shell would.
package words

import (
	"fmt"
	"strings"
)

// Split splits s into words as a shell would: on blanks, with '...'
// taken as written, "..." too but for \" and \\, and a \ outside
// quotes standing for the character after it.
func Split(s string) ([]string, error) {
	var words []string
	var w strings.Builder
	inWord := false
	var quote byte
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case quote != 0 && c == quote:
			quote = 0
		case quote == '\'':
			w.WriteByte(c)
		case c == '\\':
			if i+1 == len(s) {
				return nil, fmt.Errorf("trailing \\")
			}
			if quote == '"' && s[i+1] != '"' && s[i+1] != '\\' {
				w.WriteByte(c)
				continue
			}
			i++
			w.WriteByte(s[i])
			inWord = true
		case quote == '"':
			w.WriteByte(c)
		case c == '\'' || c == '"':
			quote = c
			inWord = true
		case c == ' ' || c == '\t' || c == '\n':
			if inWord {
				words = append(words, w.String())
				w.Reset()
				inWord = false
			}
		default:
			w.WriteByte(c)
			inWord = true
		}
	}
	if quote != 0 {
		return nil, fmt.Errorf("unclosed %c", quote)
	}
	if inWord {
		words = append(words, w.String())
	}
	return words, nil
}
