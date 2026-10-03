package words

import (
	"reflect"
	"testing"
)

// TestSplit: words split on blanks, '...' taken as written, "..." too
// but for \" and \\, and \ outside quotes standing for the character
// after it.
func TestSplit(t *testing.T) {
	for _, tc := range []struct {
		in   string
		want []string
	}{
		{"", nil},
		{" \t\n ", nil},
		{"-F -S", []string{"-F", "-S"}},
		{"bat --color=always %s", []string{"bat", "--color=always", "%s"}},
		{"a 'b c' d", []string{"a", "b c", "d"}},
		{`a "b c" d`, []string{"a", "b c", "d"}},
		{`--x="y z"`, []string{"--x=y z"}},
		{`my\ file`, []string{"my file"}},
		{`"a \"b\" \\ \c"`, []string{`a "b" \ \c`}},
		{`'a\b'`, []string{`a\b`}},
		{"''", []string{""}},
		{`sed -e 's/a/b/' %s`, []string{"sed", "-e", "s/a/b/", "%s"}},
	} {
		got, err := Split(tc.in)
		if err != nil || !reflect.DeepEqual(got, tc.want) {
			t.Errorf("Split(%q) = %q, %v; want %q", tc.in, got, err, tc.want)
		}
	}
	for _, tc := range []struct{ in, err string }{
		{"'a", "unclosed '"},
		{`"a`, `unclosed "`},
		{`a \`, `trailing \`},
	} {
		if _, err := Split(tc.in); err == nil || err.Error() != tc.err {
			t.Errorf("Split(%q) err = %v, want %s", tc.in, err, tc.err)
		}
	}
}
