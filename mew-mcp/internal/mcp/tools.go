package mcpserver

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	"github.com/tripplen23/mew/mew-mcp/internal/mew"
)

const (
	ServerName    = "mew-mcp"
	ServerVersion = "0.1.0"
)

// NewServer creates an official MCP SDK server and registers the mew workflow tools.
func NewServer(client *mew.Client) *mcp.Server {
	server := mcp.NewServer(&mcp.Implementation{Name: ServerName, Version: ServerVersion}, nil)
	RegisterTools(server, client)
	return server
}

// RegisterTools binds workflow-level mew tools to the official MCP Go SDK server.
func RegisterTools(server *mcp.Server, client *mew.Client) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        "mew_health",
		Description: "Check if the mewcode server is running and healthy. Returns service name and version. Use this first to verify connectivity before calling other tools.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in HealthInput) (*mcp.CallToolResult, HealthOutput, error) {
		h, err := client.Health(ctx)
		if err != nil {
			return nil, HealthOutput{}, err
		}
		return nil, HealthOutput{Ok: h.Ok, Service: h.Service, Version: h.Version}, nil
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "ask_mew",
		Description: "Send a prompt to mew in BUILD mode (read+write tools). Creates a new session, sends the message, and returns the full assistant reply. Use this for one-shot coding or file-editing tasks.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in AskMewInput) (*mcp.CallToolResult, AskMewOutput, error) {
		if in.Prompt == "" {
			return nil, AskMewOutput{}, fmt.Errorf("prompt is required")
		}
		title := in.Title
		if title == "" {
			title = defaultTitle(in.Prompt)
		}
		session, err := client.CreateSession(ctx, title, in.Model, "BUILD")
		if err != nil {
			return nil, AskMewOutput{}, fmt.Errorf("create session: %w", err)
		}
		result, err := client.Chat(ctx, session.ID, session.Model, "BUILD", in.Prompt)
		if err != nil {
			return nil, AskMewOutput{}, fmt.Errorf("chat: %w", err)
		}
		return nil, AskMewOutput{SessionID: session.ID, Reply: result.Text, Finished: result.Finished, Error: result.Error}, nil
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "continue_mew_session",
		Description: "Send a follow-up message to an existing mew session in BUILD mode. Returns the full assistant reply. Use this to continue a conversation started by ask_mew.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in ContinueMewInput) (*mcp.CallToolResult, ContinueMewOutput, error) {
		if in.SessionID == "" {
			return nil, ContinueMewOutput{}, fmt.Errorf("session_id is required")
		}
		if in.Prompt == "" {
			return nil, ContinueMewOutput{}, fmt.Errorf("prompt is required")
		}
		model := in.Model
		if model == "" {
			s, err := client.GetSession(ctx, in.SessionID)
			if err != nil {
				return nil, ContinueMewOutput{}, fmt.Errorf("get session for model: %w", err)
			}
			model = s.Model
		}
		result, err := client.Chat(ctx, in.SessionID, model, "BUILD", in.Prompt)
		if err != nil {
			return nil, ContinueMewOutput{}, fmt.Errorf("chat: %w", err)
		}
		return nil, ContinueMewOutput{SessionID: in.SessionID, Reply: result.Text, Finished: result.Finished, Error: result.Error}, nil
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "list_mew_sessions",
		Description: "List all mew sessions (newest first), showing id, title, model, mode, and creation time. Use this to find existing sessions to continue.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in ListSessionsInput) (*mcp.CallToolResult, ListSessionsOutput, error) {
		sessions, err := client.ListSessions(ctx)
		if err != nil {
			return nil, ListSessionsOutput{}, err
		}
		return nil, ListSessionsOutput{Sessions: sessions}, nil
	})

	mcp.AddTool(server, &mcp.Tool{
		Name:        "get_mew_session",
		Description: "Get full details of a mew session including its complete message history. Use this to review what was discussed in a session.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in GetSessionInput) (*mcp.CallToolResult, GetSessionOutput, error) {
		if in.SessionID == "" {
			return nil, GetSessionOutput{}, fmt.Errorf("session_id is required")
		}
		s, err := client.GetSession(ctx, in.SessionID)
		if err != nil {
			return nil, GetSessionOutput{}, err
		}
		return nil, GetSessionOutput{Session: *s}, nil
	})
}

func defaultTitle(prompt string) string {
	r := []rune(prompt)
	if len(r) > 50 {
		return string(r[:50]) + "..."
	}
	return prompt
}

// Tool input/output structs. jsonschema tags are consumed by the official MCP Go SDK.

type HealthInput struct{}

type HealthOutput struct {
	Ok      bool   `json:"ok" jsonschema:"whether mewcode-server is healthy"`
	Service string `json:"service" jsonschema:"service name reported by mewcode-server"`
	Version string `json:"version" jsonschema:"mewcode-server version"`
}

type AskMewInput struct {
	Prompt string `json:"prompt" jsonschema:"the user's prompt to send to mew"`
	Title  string `json:"title,omitempty" jsonschema:"optional session title; defaults to a truncated prompt"`
	Model  string `json:"model,omitempty" jsonschema:"optional model id, such as minimax-m3 or glm-5.1"`
}

type AskMewOutput struct {
	SessionID string `json:"session_id" jsonschema:"new mew session UUID"`
	Reply     string `json:"reply" jsonschema:"assistant reply text"`
	Finished  bool   `json:"finished" jsonschema:"whether the stream reached a finish event"`
	Error     string `json:"error,omitempty" jsonschema:"stream error message, if any"`
}

type ContinueMewInput struct {
	SessionID string `json:"session_id" jsonschema:"existing mew session UUID"`
	Prompt    string `json:"prompt" jsonschema:"follow-up message to send to the session"`
	Model     string `json:"model,omitempty" jsonschema:"optional model id; defaults to the session model"`
}

type ContinueMewOutput struct {
	SessionID string `json:"session_id" jsonschema:"mew session UUID"`
	Reply     string `json:"reply" jsonschema:"assistant reply text"`
	Finished  bool   `json:"finished" jsonschema:"whether the stream reached a finish event"`
	Error     string `json:"error,omitempty" jsonschema:"stream error message, if any"`
}

type ListSessionsInput struct{}

type ListSessionsOutput struct {
	Sessions []mew.SessionSummary `json:"sessions" jsonschema:"mew sessions, newest first"`
}

type GetSessionInput struct {
	SessionID string `json:"session_id" jsonschema:"mew session UUID"`
}

type GetSessionOutput struct {
	Session mew.Session `json:"session" jsonschema:"full mew session including message history"`
}

// JSONString is useful when clients prefer a raw JSON text representation.
func JSONString(v any) string {
	b, _ := json.Marshal(v)
	return string(b)
}
