// Command mew-mcp is the MCP stdio server that exposes mew workflow tools
// to external agents (Claude, Cursor, Hermes).
//
// Usage:
//
//	mew-mcp                    # uses MEWCODE_API_URL or http://127.0.0.1:3737
//	mew-mcp --url http://...   # explicit URL
//
// The server uses the official MCP Go SDK (github.com/modelcontextprotocol/go-sdk)
// and runs over stdio transport. Logs go to stderr.
package main

import (
	"context"
	"flag"
	"log"
	"os"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	mcpserver "github.com/tripplen23/mew/mew-mcp/internal/mcp"
	"github.com/tripplen23/mew/mew-mcp/internal/mew"
)

func main() {
	url := flag.String("url", "", "mewcode-server URL (overrides MEWCODE_API_URL)")
	flag.Parse()

	addr := *url
	if addr == "" {
		addr = os.Getenv("MEWCODE_API_URL")
	}
	if addr == "" {
		addr = "http://127.0.0.1:3737"
	}

	client := mew.NewClient(addr)
	server := mcpserver.NewServer(client)

	log.SetOutput(os.Stderr)
	log.Printf("mew-mcp starting — backend: %s", addr)
	if err := server.Run(context.Background(), &mcp.StdioTransport{}); err != nil {
		log.Fatal(err)
	}
}
