// Package mew provides a minimal HTTP client for the mewcode-server REST API.
//
// The client wraps the endpoints documented in the OpenAPI spec at
// /api-docs/openapi.json: health, sessions CRUD, and the SSE chat stream.
// It is deliberately dependency-free (stdlib only) so the MCP server can
// be built with any Go toolchain ≥ 1.22.
package mew

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// Client talks to a running mewcode-server instance.
type Client struct {
	BaseURL string
	HTTP    *http.Client
}

// NewClient creates a client targeting the given base URL (e.g.
// "http://127.0.0.1:3737"). Trailing slashes are stripped.
func NewClient(baseURL string) *Client {
	return &Client{
		BaseURL: strings.TrimRight(baseURL, "/"),
		HTTP:    &http.Client{Timeout: 30 * time.Second},
	}
}

// HealthResponse is the body of GET /health.
type HealthResponse struct {
	Ok       bool   `json:"ok"`
	Service  string `json:"service"`
	Version  string `json:"version"`
}

// Health calls GET /health.
func (c *Client) Health() (*HealthResponse, error) {
	resp, err := c.HTTP.Get(c.BaseURL + "/health")
	if err != nil {
		return nil, fmt.Errorf("health: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("health: status %d", resp.StatusCode)
	}
	var h HealthResponse
	if err := json.NewDecoder(resp.Body).Decode(&h); err != nil {
		return nil, fmt.Errorf("health: decode: %w", err)
	}
	return &h, nil
}

// SessionSummary is the lightweight view returned by GET /sessions.
type SessionSummary struct {
	ID        string `json:"id"`
	Title     string `json:"title"`
	Model     string `json:"model"`
	Mode      string `json:"mode"`
	CreatedAt string `json:"created_at"`
}

// Session is the full session with message history (GET /sessions/{id}).
type Session struct {
	ID        string   `json:"id"`
	Title     string   `json:"title"`
	Model     string   `json:"model"`
	Mode      string   `json:"mode"`
	CreatedAt string   `json:"created_at"`
	UpdatedAt string   `json:"updated_at"`
	Messages  []Message `json:"messages"`
}

// Message mirrors mewcode-protocol::Message.
type Message struct {
	ID        string        `json:"id"`
	Role      string        `json:"role"`
	Parts     []MessagePart `json:"parts"`
	Model     string        `json:"model,omitempty"`
	CreatedAt string        `json:"created_at"`
}

// MessagePart mirrors mewcode-protocol::MessagePart (tagged enum).
type MessagePart struct {
	Type string `json:"type"`
	Text string `json:"text,omitempty"`
}

// ListSessions calls GET /sessions.
func (c *Client) ListSessions() ([]SessionSummary, error) {
	resp, err := c.HTTP.Get(c.BaseURL + "/sessions")
	if err != nil {
		return nil, fmt.Errorf("list sessions: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("list sessions: status %d", resp.StatusCode)
	}
	var sessions []SessionSummary
	if err := json.NewDecoder(resp.Body).Decode(&sessions); err != nil {
		return nil, fmt.Errorf("list sessions: decode: %w", err)
	}
	return sessions, nil
}

// CreateSession calls POST /sessions. model and mode may be empty to use
// server defaults.
func (c *Client) CreateSession(title, model, mode string) (*Session, error) {
	body := map[string]any{"title": title}
	if model != "" {
		body["model"] = model
	}
	if mode != "" {
		body["mode"] = mode
	}
	payload, _ := json.Marshal(body)
	resp, err := c.HTTP.Post(c.BaseURL+"/sessions", "application/json", bytes.NewReader(payload))
	if err != nil {
		return nil, fmt.Errorf("create session: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 201 {
		return nil, fmt.Errorf("create session: status %d", resp.StatusCode)
	}
	var s Session
	if err := json.NewDecoder(resp.Body).Decode(&s); err != nil {
		return nil, fmt.Errorf("create session: decode: %w", err)
	}
	return &s, nil
}

// GetSession calls GET /sessions/{id}.
func (c *Client) GetSession(id string) (*Session, error) {
	resp, err := c.HTTP.Get(c.BaseURL + "/sessions/" + id)
	if err != nil {
		return nil, fmt.Errorf("get session: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("get session: status %d", resp.StatusCode)
	}
	var s Session
	if err := json.NewDecoder(resp.Body).Decode(&s); err != nil {
		return nil, fmt.Errorf("get session: decode: %w", err)
	}
	return &s, nil
}

// ChatResult is the accumulated output of a completed chat turn.
type ChatResult struct {
	Text     string
	Finished bool
	Error    string
}

// Chat calls POST /chat and consumes the SSE stream until a finish or error
// event arrives. The accumulated assistant text is returned.
func (c *Client) Chat(sessionID, model, mode, userText string) (*ChatResult, error) {
	body := map[string]any{
		"session_id": sessionID,
		"model":      model,
		"mode":       mode,
		"messages": []map[string]any{
			{
				"id":         "00000000-0000-0000-0000-000000000000",
				"role":       "user",
				"parts":      []map[string]any{{"type": "text", "text": userText}},
				"created_at": "2025-01-01T00:00:00Z",
			},
		},
	}
	payload, _ := json.Marshal(body)
	// Use a client with no timeout for streaming.
	transport := c.HTTP.Transport
	if transport == nil {
		transport = http.DefaultTransport
	}
	streamClient := &http.Client{Transport: transport}
	resp, err := streamClient.Post(c.BaseURL+"/chat", "application/json", bytes.NewReader(payload))
	if err != nil {
		return nil, fmt.Errorf("chat: %w", err)
	}
	defer resp.Body.Close()

	result := &ChatResult{}
	scanner := bufio.NewScanner(resp.Body)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.HasPrefix(line, "data: ") {
			continue
		}
		data := strings.TrimPrefix(line, "data: ")
		var event map[string]any
		if err := json.Unmarshal([]byte(data), &event); err != nil {
			continue
		}
		switch event["type"] {
		case "text-delta":
			if delta, ok := event["delta"].(string); ok {
				result.Text += delta
			}
		case "finish":
			result.Finished = true
		case "error":
			if msg, ok := event["message"].(string); ok {
				result.Error = msg
			}
			result.Finished = true
		}
	}
	if err := scanner.Err(); err != nil && err != io.EOF {
		return nil, fmt.Errorf("chat: scan: %w", err)
	}
	return result, nil
}
