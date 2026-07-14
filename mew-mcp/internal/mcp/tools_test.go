package mcp

import (
	"testing"
)

func TestToolRegistryHasExpectedTools(t *testing.T) {
	r := NewToolRegistry(nil)
	names := r.Names()

	expected := []string{"ask_mew", "continue_mew_session", "list_mew_sessions", "get_mew_session", "mew_health"}
	for _, want := range expected {
		found := false
		for _, n := range names {
			if n == want {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("tool registry missing %q; have %v", want, names)
		}
	}
}

func TestToolSchemaHasRequiredFields(t *testing.T) {
	r := NewToolRegistry(nil)
	for _, name := range r.Names() {
		tool, err := r.Get(name)
		if err != nil {
			t.Errorf("Get(%q) error: %v", name, err)
			continue
		}
		if tool.Description == "" {
			t.Errorf("tool %q has empty description", name)
		}
		// Every tool must declare its properties
		if len(tool.InputSchema.Properties) == 0 && len(tool.InputSchema.Required) == 0 {
			// mew_health has no params — that's fine, but others must have properties
			if name != "mew_health" && name != "list_mew_sessions" {
				t.Errorf("tool %q has no input properties and no required fields", name)
			}
		}
	}
}
