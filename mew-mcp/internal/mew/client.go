// Package mew provides a minimal HTTP client for the mewcode-server REST API.
package mew

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
)

// Client talks to a running mewcode-server instance.
type Client struct {
	BaseURL string
	HTTP    *http.Client
}

// NewClient creates a client targeting the given base URL (e.g. "http://127.0.0.1:3737").
func NewClient(baseURL string) *Client {
	return &Client{
		BaseURL: strings.TrimRight(baseURL, "/"),
		HTTP:    &http.Client{Timeout: 30 * time.Second},
	}
}

type HealthResponse struct {
	Ok      bool   `json:"ok" jsonschema:"whether mewcode-server is healthy"`
	Service string `json:"service" jsonschema:"service name"`
	Version string `json:"version" jsonschema:"service version"`
}

func (c *Client) Health(ctx context.Context) (*HealthResponse, error) {
	var h HealthResponse
	if err := c.getJSON(ctx, "/health", &h); err != nil {
		return nil, fmt.Errorf("health: %w", err)
	}
	return &h, nil
}

type SessionSummary struct {
	ID        string `json:"id" jsonschema:"session UUID"`
	Title     string `json:"title" jsonschema:"session title"`
	Model     string `json:"model" jsonschema:"model id"`
	Mode      string `json:"mode" jsonschema:"session mode"`
	CreatedAt string `json:"created_at" jsonschema:"creation timestamp"`
}

type Session struct {
	ID        string    `json:"id" jsonschema:"session UUID"`
	Title     string    `json:"title" jsonschema:"session title"`
	Model     string    `json:"model" jsonschema:"model id"`
	Mode      string    `json:"mode" jsonschema:"session mode"`
	CreatedAt string    `json:"created_at" jsonschema:"creation timestamp"`
	UpdatedAt string    `json:"updated_at" jsonschema:"last update timestamp"`
	Messages  []Message `json:"messages" jsonschema:"session message history"`
}

type Message struct {
	ID        string        `json:"id" jsonschema:"message UUID"`
	Role      string        `json:"role" jsonschema:"message role: user, assistant, or tool"`
	Parts     []MessagePart `json:"parts" jsonschema:"ordered message parts"`
	Model     string        `json:"model,omitempty" jsonschema:"assistant model id"`
	CreatedAt string        `json:"created_at" jsonschema:"creation timestamp"`
}

type MessagePart struct {
	Type string `json:"type" jsonschema:"part type, such as text"`
	Text string `json:"text,omitempty" jsonschema:"text content when type is text"`
}

func (c *Client) ListSessions(ctx context.Context) ([]SessionSummary, error) {
	var sessions []SessionSummary
	if err := c.getJSON(ctx, "/sessions", &sessions); err != nil {
		return nil, fmt.Errorf("list sessions: %w", err)
	}
	return sessions, nil
}

func (c *Client) CreateSession(ctx context.Context, title, model, mode string) (*Session, error) {
	body := map[string]any{"title": title}
	if model != "" {
		body["model"] = model
	}
	if mode != "" {
		body["mode"] = mode
	}
	var s Session
	if err := c.postJSON(ctx, "/sessions", body, http.StatusCreated, &s); err != nil {
		return nil, fmt.Errorf("create session: %w", err)
	}
	return &s, nil
}

func (c *Client) GetSession(ctx context.Context, id string) (*Session, error) {
	var s Session
	if err := c.getJSON(ctx, "/sessions/"+id, &s); err != nil {
		return nil, fmt.Errorf("get session: %w", err)
	}
	return &s, nil
}

type ChatResult struct {
	Text     string
	Finished bool
	Error    string
}

func (c *Client) Chat(ctx context.Context, sessionID, model, mode, userText string) (*ChatResult, error) {
	msgID := uuid.NewString()
	createdAt := time.Now().UTC().Format(time.RFC3339)
	body := map[string]any{
		"session_id": sessionID,
		"model":      model,
		"mode":       mode,
		"messages": []map[string]any{{
			"id":         msgID,
			"role":       "user",
			"parts":      []map[string]any{{"type": "text", "text": userText}},
			"created_at": createdAt,
		}},
	}
	payload, _ := json.Marshal(body)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.BaseURL+"/chat", bytes.NewReader(payload))
	if err != nil {
		return nil, fmt.Errorf("chat: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	streamClient := &http.Client{
		Transport: c.HTTP.Transport,
		Timeout:   5 * time.Minute,
	}
	if streamClient.Transport == nil {
		streamClient.Transport = http.DefaultTransport
	}
	resp, err := streamClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("chat: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		b, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("chat: status %d: %s", resp.StatusCode, string(b))
	}

	result := &ChatResult{}
	scanner := bufio.NewScanner(resp.Body)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.HasPrefix(line, "data: ") {
			continue
		}
		var event map[string]any
		if err := json.Unmarshal([]byte(strings.TrimPrefix(line, "data: ")), &event); err != nil {
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

func (c *Client) getJSON(ctx context.Context, path string, out any) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.BaseURL+path, nil)
	if err != nil {
		return err
	}
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("status %d", resp.StatusCode)
	}
	return json.NewDecoder(resp.Body).Decode(out)
}

func (c *Client) postJSON(ctx context.Context, path string, body any, wantStatus int, out any) error {
	payload, _ := json.Marshal(body)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.BaseURL+path, bytes.NewReader(payload))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != wantStatus {
		return fmt.Errorf("status %d", resp.StatusCode)
	}
	return json.NewDecoder(resp.Body).Decode(out)
}
