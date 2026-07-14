package mew

import (
	"context"
	"fmt"
	"net/http"
)

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
