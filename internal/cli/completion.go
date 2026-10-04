package cli

import _ "embed"

var (
	//go:embed completions/vedi.bash
	bashCompletion string
	//go:embed completions/_vedi
	zshCompletion string
	//go:embed completions/vedi.fish
	fishCompletion string
)

// Completion is the completion script for a shell, "" for one vedi
// has none for.
func Completion(shell string) string {
	switch shell {
	case "bash":
		return bashCompletion
	case "zsh":
		return zshCompletion
	case "fish":
		return fishCompletion
	}
	return ""
}
