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

const streamHTTPTimeout = 5 * time.Minute

type ChatResult struct {
	Text     string
	Finished bool
	Error    string
}

type chatRequest struct {
	SessionID string    `json:"session_id"`
	Model     string    `json:"model"`
	Mode      string    `json:"mode"`
	Messages  []Message `json:"messages"`
}

type chatEvent struct {
	Type    string `json:"type"`
	Delta   string `json:"delta"`
	Message string `json:"message"`
}

func (c *Client) Chat(ctx context.Context, sessionID, model, mode, userText string) (*ChatResult, error) {
	body := chatRequest{
		SessionID: sessionID,
		Model:     model,
		Mode:      mode,
		Messages: []Message{{
			ID:        uuid.NewString(),
			Role:      "user",
			Parts:     []MessagePart{{Type: "text", Text: userText}},
			CreatedAt: time.Now().UTC().Format(time.RFC3339),
		}},
	}
	payload, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("chat: %w", err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.BaseURL+"/chat", bytes.NewReader(payload))
	if err != nil {
		return nil, fmt.Errorf("chat: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")

	streamClient := &http.Client{Transport: c.HTTP.Transport, Timeout: streamHTTPTimeout}
	if streamClient.Transport == nil {
		streamClient.Transport = http.DefaultTransport
	}
	resp, err := streamClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("chat: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, statusError("chat", resp.StatusCode, http.StatusOK, resp.Body)
	}

	result := &ChatResult{}
	scanner := bufio.NewScanner(resp.Body)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.HasPrefix(line, "data: ") {
			continue
		}
		var event chatEvent
		if err := json.Unmarshal([]byte(strings.TrimPrefix(line, "data: ")), &event); err != nil {
			continue
		}
		switch event.Type {
		case "text-delta":
			result.Text += event.Delta
		case "finish":
			result.Finished = true
		case "error":
			result.Error = event.Message
			result.Finished = true
		}
	}
	if err := scanner.Err(); err != nil && err != io.EOF {
		return nil, fmt.Errorf("chat: scan: %w", err)
	}
	return result, nil
}
