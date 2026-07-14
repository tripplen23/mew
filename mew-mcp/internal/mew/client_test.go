package mew

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestHealth(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"ok":true,"service":"mewcode-server","version":"0.1.0"}`))
	}))
	defer srv.Close()

	c := NewClient(srv.URL)
	h, err := c.Health(context.Background())
	if err != nil {
		t.Fatalf("Health() error: %v", err)
	}
	if !h.Ok {
		t.Error("Health().Ok = false, want true")
	}
	if h.Service != "mewcode-server" {
		t.Errorf("Health().Service = %q, want %q", h.Service, "mewcode-server")
	}
}

func TestListSessions(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/sessions" || r.Method != "GET" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`[{"id":"550e8400-e29b-41d4-a716-446655440000","title":"test","model":"minimax-m3","mode":"BUILD","created_at":"2025-01-01T00:00:00Z"}]`))
	}))
	defer srv.Close()

	c := NewClient(srv.URL)
	sessions, err := c.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions() error: %v", err)
	}
	if len(sessions) != 1 {
		t.Fatalf("ListSessions() returned %d sessions, want 1", len(sessions))
	}
	if sessions[0].Title != "test" {
		t.Errorf("sessions[0].Title = %q, want %q", sessions[0].Title, "test")
	}
}

func TestCreateSession(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/sessions" || r.Method != "POST" {
			http.NotFound(w, r)
			return
		}
		var body map[string]any
		json.NewDecoder(r.Body).Decode(&body)
		if body["title"] != "hello" {
			t.Errorf("title = %v, want %q", body["title"], "hello")
		}
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte(`{"id":"550e8400-e29b-41d4-a716-446655440000","title":"hello","model":"minimax-m3","mode":"BUILD","created_at":"2025-01-01T00:00:00Z","updated_at":"2025-01-01T00:00:00Z","messages":[]}`))
	}))
	defer srv.Close()

	c := NewClient(srv.URL)
	s, err := c.CreateSession(context.Background(), "hello", "", "")
	if err != nil {
		t.Fatalf("CreateSession() error: %v", err)
	}
	if s.Title != "hello" {
		t.Errorf("CreateSession().Title = %q, want %q", s.Title, "hello")
	}
	if s.ID == "" {
		t.Error("CreateSession().ID is empty")
	}
}

func TestGetSession(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/sessions/550e8400-e29b-41d4-a716-446655440000" && r.Method == "GET" {
			w.Header().Set("Content-Type", "application/json")
			w.Write([]byte(`{"id":"550e8400-e29b-41d4-a716-446655440000","title":"detail","model":"glm-5.1","mode":"PLAN","created_at":"2025-01-01T00:00:00Z","updated_at":"2025-01-01T00:00:00Z","messages":[{"id":"660e8400-e29b-41d4-a716-446655440001","role":"user","parts":[{"type":"text","text":"hi"}],"created_at":"2025-01-01T00:00:00Z"}]}`))
			return
		}
		http.NotFound(w, r)
	}))
	defer srv.Close()

	c := NewClient(srv.URL)
	s, err := c.GetSession(context.Background(), "550e8400-e29b-41d4-a716-446655440000")
	if err != nil {
		t.Fatalf("GetSession() error: %v", err)
	}
	if s.Title != "detail" {
		t.Errorf("GetSession().Title = %q, want %q", s.Title, "detail")
	}
	if len(s.Messages) != 1 {
		t.Fatalf("GetSession() messages = %d, want 1", len(s.Messages))
	}
	if s.Messages[0].Role != "user" {
		t.Errorf("Messages[0].Role = %q, want %q", s.Messages[0].Role, "user")
	}
}

func TestChat(t *testing.T) {
	// SSE response: two events then a finish
	sseBody := `data: {"type":"start","message_id":"550e8400-e29b-41d4-a716-446655440000","mode":"BUILD","model":"minimax-m3"}

data: {"type":"text-delta","delta":"Hello "}

data: {"type":"text-delta","delta":"world"}

data: {"type":"finish","duration_ms":42}

`
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat" || r.Method != "POST" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.Write([]byte(sseBody))
	}))
	defer srv.Close()

	c := NewClient(srv.URL)
	result, err := c.Chat(context.Background(), "550e8400-e29b-41d4-a716-446655440000", "minimax-m3", "BUILD", "Hello")
	if err != nil {
		t.Fatalf("Chat() error: %v", err)
	}
	if result.Text != "Hello world" {
		t.Errorf("Chat() text = %q, want %q", result.Text, "Hello world")
	}
	if !result.Finished {
		t.Error("Chat() finished = false, want true")
	}
}
