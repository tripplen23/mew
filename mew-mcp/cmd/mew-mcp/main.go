// Command mew-mcp is the MCP stdio server that exposes mew workflow tools
// to external agents (Claude, Cursor, Hermes).
//
// Usage:
//
//	mew-mcp                    # uses MEWCODE_API_URL or http://127.0.0.1:3737
//	mew-mcp --url http://...   # explicit URL
//
// The server reads JSON-RPC 2.0 messages from stdin and writes responses
// to stdout. Logs go to stderr so they don't interfere with the protocol.
package main

import (
	"flag"
	"log"
	"os"

	"github.com/tripplen23/mew/mew-mcp/internal/mcp"
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
	server := mcp.NewServer(client)

	log.SetOutput(os.Stderr)
	log.Printf("mew-mcp starting — backend: %s", addr)
	server.Serve(os.Stdin, os.Stdout)
}
