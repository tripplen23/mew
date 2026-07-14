// Package mcp implements a minimal MCP (Model Context Protocol) stdio server
// using JSON-RPC 2.0 over stdin/stdout — no external SDK dependency.
//
// The server exposes mew workflow tools so external agents (Claude, Cursor,
// Hermes) can interact with a running mewcode-server.
package mcp

import (
	"encoding/json"
	"fmt"

	"github.com/tripplen23/mew/mew-mcp/internal/mew"
)

// ToolSchema describes a tool's input schema (JSON Schema subset).
type ToolSchema struct {
	Type       string             `json:"type"`
	Properties map[string]SchemaField `json:"properties"`
	Required   []string           `json:"required"`
}

// SchemaField is one property in a ToolSchema.
type SchemaField struct {
	Type        string `json:"type"`
	Description string `json:"description"`
}

// Tool represents a single MCP tool.
type Tool struct {
	Name        string
	Description string
	InputSchema ToolSchema
	Handler     func(client *mew.Client, args map[string]any) (string, error)
}

// ToolRegistry holds all registered tools.
type ToolRegistry struct {
	tools map[string]Tool
	order []string
}

// NewToolRegistry creates a registry with the five workflow-level tools
// that mew-mcp exposes to external agents.
func NewToolRegistry(client *mew.Client) *ToolRegistry {
	r := &ToolRegistry{tools: make(map[string]Tool)}

	r.register(Tool{
		Name:        "mew_health",
		Description: "Check if the mewcode server is running and healthy. Returns service name and version. Use this first to verify connectivity before calling other tools.",
		InputSchema: ToolSchema{
			Type:       "object",
			Properties: map[string]SchemaField{},
			Required:   nil,
		},
		Handler: func(c *mew.Client, _ map[string]any) (string, error) {
			h, err := c.Health()
			if err != nil {
				return "", err
			}
			b, _ := json.Marshal(h)
			return string(b), nil
		},
	})

	r.register(Tool{
		Name:        "ask_mew",
		Description: "Send a prompt to mew in BUILD mode (read+write tools). Creates a new session, sends the message, and returns the full assistant reply. Use this for one-shot coding or file-editing tasks.",
		InputSchema: ToolSchema{
			Type: "object",
			Properties: map[string]SchemaField{
				"prompt": {Type: "string", Description: "The user's prompt to send to mew."},
				"title":  {Type: "string", Description: "Optional session title. Defaults to a truncated version of the prompt."},
				"model":  {Type: "string", Description: "Model id (e.g. \"minimax-m3\", \"glm-5.1\"). Defaults to server default."},
			},
			Required: []string{"prompt"},
		},
		Handler: handleAskMew,
	})

	r.register(Tool{
		Name:        "continue_mew_session",
		Description: "Send a follow-up message to an existing mew session in BUILD mode. Returns the full assistant reply. Use this to continue a conversation started by ask_mew.",
		InputSchema: ToolSchema{
			Type: "object",
			Properties: map[string]SchemaField{
				"session_id": {Type: "string", Description: "The session UUID to continue."},
				"prompt":     {Type: "string", Description: "The follow-up message."},
				"model":      {Type: "string", Description: "Model id. Defaults to the session's current model."},
			},
			Required: []string{"session_id", "prompt"},
		},
		Handler: handleContinueMew,
	})

	r.register(Tool{
		Name:        "list_mew_sessions",
		Description: "List all mew sessions (newest first), showing id, title, model, mode, and creation time. Use this to find existing sessions to continue.",
		InputSchema: ToolSchema{
			Type:       "object",
			Properties: map[string]SchemaField{},
			Required:   nil,
		},
		Handler: func(c *mew.Client, _ map[string]any) (string, error) {
			sessions, err := c.ListSessions()
			if err != nil {
				return "", err
			}
			b, _ := json.Marshal(sessions)
			return string(b), nil
		},
	})

	r.register(Tool{
		Name:        "get_mew_session",
		Description: "Get full details of a mew session including its complete message history. Use this to review what was discussed in a session.",
		InputSchema: ToolSchema{
			Type: "object",
			Properties: map[string]SchemaField{
				"session_id": {Type: "string", Description: "The session UUID to retrieve."},
			},
			Required: []string{"session_id"},
		},
		Handler: func(c *mew.Client, args map[string]any) (string, error) {
			id, _ := args["session_id"].(string)
			if id == "" {
				return "", fmt.Errorf("session_id is required")
			}
			s, err := c.GetSession(id)
			if err != nil {
				return "", err
			}
			b, _ := json.Marshal(s)
			return string(b), nil
		},
	})

	return r
}

func (r *ToolRegistry) register(t Tool) {
	r.tools[t.Name] = t
	r.order = append(r.order, t.Name)
}

// Names returns tool names in registration order.
func (r *ToolRegistry) Names() []string {
	return r.order
}

// Get returns a tool by name.
func (r *ToolRegistry) Get(name string) (Tool, error) {
	t, ok := r.tools[name]
	if !ok {
		return Tool{}, fmt.Errorf("unknown tool: %s", name)
	}
	return t, nil
}

// --- Tool handlers ---

func handleAskMew(c *mew.Client, args map[string]any) (string, error) {
	prompt, _ := args["prompt"].(string)
	if prompt == "" {
		return "", fmt.Errorf("prompt is required")
	}
	title, _ := args["title"].(string)
	if title == "" {
		if len(prompt) > 50 {
			title = prompt[:50] + "..."
		} else {
			title = prompt
		}
	}
	model, _ := args["model"].(string)

	session, err := c.CreateSession(title, model, "BUILD")
	if err != nil {
		return "", fmt.Errorf("create session: %w", err)
	}
	result, err := c.Chat(session.ID, session.Model, "BUILD", prompt)
	if err != nil {
		return "", fmt.Errorf("chat: %w", err)
	}
	out := map[string]any{
		"session_id": session.ID,
		"reply":      result.Text,
		"finished":   result.Finished,
	}
	if result.Error != "" {
		out["error"] = result.Error
	}
	b, _ := json.Marshal(out)
	return string(b), nil
}

func handleContinueMew(c *mew.Client, args map[string]any) (string, error) {
	sessionID, _ := args["session_id"].(string)
	if sessionID == "" {
		return "", fmt.Errorf("session_id is required")
	}
	prompt, _ := args["prompt"].(string)
	if prompt == "" {
		return "", fmt.Errorf("prompt is required")
	}
	model, _ := args["model"].(string)
	if model == "" {
		// Fetch session to get its model
		s, err := c.GetSession(sessionID)
		if err != nil {
			return "", fmt.Errorf("get session for model: %w", err)
		}
		model = s.Model
	}

	result, err := c.Chat(sessionID, model, "BUILD", prompt)
	if err != nil {
		return "", fmt.Errorf("chat: %w", err)
	}
	out := map[string]any{
		"session_id": sessionID,
		"reply":      result.Text,
		"finished":   result.Finished,
	}
	if result.Error != "" {
		out["error"] = result.Error
	}
	b, _ := json.Marshal(out)
	return string(b), nil
}
