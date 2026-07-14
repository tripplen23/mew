package mcp

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"strings"

	"github.com/tripplen23/mew/mew-mcp/internal/mew"
)

// JSON-RPC 2.0 wire types.

type rpcRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type rpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

// ToolListEntry is one entry in the tools/list response.
type ToolListEntry struct {
	Name        string     `json:"name"`
	Description string     `json:"description"`
	InputSchema ToolSchema `json:"inputSchema"`
}

// ToolCallParams is the params object for tools/call.
type ToolCallParams struct {
	Name string         `json:"name"`
	Args map[string]any `json:"arguments"`
}

// Server is a stdio MCP server.
type Server struct {
	registry *ToolRegistry
	client   *mew.Client
}

// NewServer creates a new MCP stdio server.
func NewServer(client *mew.Client) *Server {
	return &Server{
		registry: NewToolRegistry(client),
		client:   client,
	}
}

// Serve reads JSON-RPC requests from stdin and writes responses to stdout.
// It blocks until EOF or a fatal error.
func (s *Server) Serve(in io.Reader, out io.Writer) {
	scanner := bufio.NewScanner(in)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	writer := bufio.NewWriter(out)
	defer writer.Flush()

	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var req rpcRequest
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			// Not valid JSON — skip silently per MCP transport convention.
			continue
		}
		resp := s.handleRequest(&req)
		if resp == nil {
			// Notification (no id) — no response.
			continue
		}
		b, err := json.Marshal(resp)
		if err != nil {
			log.Printf("marshal response: %v", err)
			continue
		}
		writer.Write(b)
		writer.WriteByte('\n')
		writer.Flush()
	}
	if err := scanner.Err(); err != nil && err != io.EOF {
		log.Printf("scanner: %v", err)
	}
}

func (s *Server) handleRequest(req *rpcRequest) *rpcResponse {
	// Notifications (no id) get no response.
	if len(req.ID) == 0 {
		return nil
	}

	switch req.Method {
	case "initialize":
		return s.handleInitialize(req)
	case "notifications/initialized":
		// Notification, no response.
		return nil
	case "tools/list":
		return s.handleToolsList(req)
	case "tools/call":
		return s.handleToolsCall(req)
	case "ping":
		return &rpcResponse{JSONRPC: "2.0", ID: req.ID, Result: map[string]any{}}
	default:
		return &rpcResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error:   &rpcError{Code: -32601, Message: fmt.Sprintf("method not found: %s", req.Method)},
		}
	}
}

func (s *Server) handleInitialize(req *rpcRequest) *rpcResponse {
	result := map[string]any{
		"protocolVersion": "2024-11-05",
		"capabilities": map[string]any{
			"tools": map[string]any{},
		},
		"serverInfo": map[string]any{
			"name":    "mew-mcp",
			"version": "0.1.0",
		},
	}
	return &rpcResponse{JSONRPC: "2.0", ID: req.ID, Result: result}
}

func (s *Server) handleToolsList(req *rpcRequest) *rpcResponse {
	entries := make([]ToolListEntry, 0, len(s.registry.Names()))
	for _, name := range s.registry.Names() {
		tool, _ := s.registry.Get(name)
		entries = append(entries, ToolListEntry{
			Name:        tool.Name,
			Description: tool.Description,
			InputSchema: tool.InputSchema,
		})
	}
	return &rpcResponse{
		JSONRPC: "2.0",
		ID:      req.ID,
		Result:  map[string]any{"tools": entries},
	}
}

func (s *Server) handleToolsCall(req *rpcRequest) *rpcResponse {
	var params ToolCallParams
	if err := json.Unmarshal(req.Params, &params); err != nil {
		return &rpcResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error:   &rpcError{Code: -32602, Message: fmt.Sprintf("invalid params: %v", err)},
		}
	}
	tool, err := s.registry.Get(params.Name)
	if err != nil {
		return &rpcResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error:   &rpcError{Code: -32602, Message: err.Error()},
		}
	}
	output, err := tool.Handler(s.client, params.Args)
	if err != nil {
		// MCP convention: return isError=true in the tool result content.
		return &rpcResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Result: map[string]any{
				"content": []map[string]any{
					{"type": "text", "text": fmt.Sprintf("Error: %v", err)},
				},
				"isError": true,
			},
		}
	}
	return &rpcResponse{
		JSONRPC: "2.0",
		ID:      req.ID,
		Result: map[string]any{
			"content": []map[string]any{
				{"type": "text", "text": output},
			},
			"isError": false,
		},
	}
}
