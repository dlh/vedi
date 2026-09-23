package input

import "testing"

func key(name string) Key {
	k, err := ParseKey(name)
	if err != nil {
		panic(err)
	}
	return k
}

func TestApply(t *testing.T) {
	m := Keymap{{key("h"), Command{Action: Help}}, {key("q"), Command{Action: Quit}}}
	got := m.Apply([]Binding{
		{key("h"), Command{Action: Left}},
		{key("x"), Command{Action: Help}},
		{key("q"), Command{}},
		{key("z"), Command{}},
	})
	want := Keymap{{key("h"), Command{Action: Left}}, {key("x"), Command{Action: Help}}}
	if len(got) != len(want) {
		t.Fatalf("Apply = %+v, want %+v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("binding %d = %+v, want %+v", i, got[i], want[i])
		}
	}
	if m[0].Cmd.Action != Help {
		t.Error("Apply changed its receiver")
	}
}
