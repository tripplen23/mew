package mcpserver

import (
	"context"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	"github.com/tripplen23/mew/mew-mcp/internal/mew"
)

func TestToolRegistryHasExpectedTools(t *testing.T) {
	client := mew.NewClient("http://127.0.0.1:3737")
	server := NewServer(client)

	t1, t2 := mcp.NewInMemoryTransports()
	ctx := context.Background()
	if _, err := server.Connect(ctx, t1, nil); err != nil {
		t.Fatalf("server.Connect: %v", err)
	}
	c := mcp.NewClient(&mcp.Implementation{Name: "test", Version: "0"}, nil)
	session, err := c.Connect(ctx, t2, nil)
	if err != nil {
		t.Fatalf("client.Connect: %v", err)
	}
	defer session.Close()

	want := map[string]bool{
		"mew_health":           false,
		"ask_mew":              false,
		"continue_mew_session": false,
		"list_mew_sessions":    false,
		"get_mew_session":      false,
	}
	for tool, err := range session.Tools(ctx, nil) {
		if err != nil {
			t.Fatalf("session.Tools: %v", err)
		}
		if _, ok := want[tool.Name]; ok {
			want[tool.Name] = true
		}
		if tool.Description == "" {
			t.Errorf("tool %q has empty description", tool.Name)
		}
	}
	for name, found := range want {
		if !found {
			t.Errorf("missing tool %q", name)
		}
	}
}
