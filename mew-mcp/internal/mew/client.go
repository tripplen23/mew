package mew

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

const errorBodyLimit = 4 * 1024
const defaultHTTPTimeout = 30 * time.Second

// Client talks to a running mewcode-server instance.
type Client struct {
	BaseURL string
	HTTP    *http.Client
}

// NewClient creates a client targeting the given base URL (e.g. "http://127.0.0.1:3737").
func NewClient(baseURL string) *Client {
	return &Client{
		BaseURL: strings.TrimRight(baseURL, "/"),
		HTTP:    &http.Client{Timeout: defaultHTTPTimeout},
	}
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
		return statusError("GET "+path, resp.StatusCode, http.StatusOK, resp.Body)
	}
	return json.NewDecoder(resp.Body).Decode(out)
}

func (c *Client) postJSON(ctx context.Context, path string, body any, wantStatus int, out any) error {
	payload, err := json.Marshal(body)
	if err != nil {
		return fmt.Errorf("marshal request: %w", err)
	}
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
		return statusError("POST "+path, resp.StatusCode, wantStatus, resp.Body)
	}
	return json.NewDecoder(resp.Body).Decode(out)
}

func statusError(op string, got, want int, body io.Reader) error {
	lim := io.LimitReader(body, errorBodyLimit)
	b, _ := io.ReadAll(lim)
	return fmt.Errorf("%s: status %d (expected %d): %s", op, got, want, strings.TrimSpace(string(b)))
}
