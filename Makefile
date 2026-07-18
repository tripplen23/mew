CARGO ?= cargo
GO ?= go
GOFMT ?= gofmt
CURL ?= curl

CORE_DIR = mew-core
MCP_DIR = mew-mcp
MCP_BIN = mew-mcp
SERVER_BIN = $(CORE_DIR)/target/debug/mewcode-server
CLIENT_BIN = $(CORE_DIR)/target/debug/mewcode
MEW_URL ?= http://127.0.0.1:3737
SERVER_WAIT_ATTEMPTS ?= 50

.PHONY: all build build-core build-mcp \
        run run-server run-tui \
        test test-core test-mcp \
        lint lint-core lint-mcp \
        fmt fmt-check check ci clean doc help

all: build

build: build-core build-mcp

build-core:
	cd $(CORE_DIR) && $(CARGO) build --workspace

build-mcp:
	cd $(MCP_DIR) && $(GO) build -o $(MCP_BIN) ./cmd/mew-mcp

run: build-core
	$(SERVER_BIN) \
		> /dev/null 2>&1 & \
	server_pid=$$!; \
	trap 'kill $$server_pid 2>/dev/null' EXIT INT TERM; \
	attempts=0; \
	until $(CURL) -sf $(MEW_URL)/health >/dev/null 2>&1; do \
		attempts=$$((attempts + 1)); \
		if [ $$attempts -ge $(SERVER_WAIT_ATTEMPTS) ]; then \
			printf '%s\n' 'server did not start; run `make run-server` for logs'; \
			exit 1; \
		fi; \
		sleep 0.3; \
	done; \
	$(CLIENT_BIN) tui

run-server: build-core
	$(SERVER_BIN)

run-tui: build-core
	$(CLIENT_BIN) tui

test: test-core test-mcp

test-core:
	cd $(CORE_DIR) && $(CARGO) test --workspace

test-mcp:
	cd $(MCP_DIR) && $(GO) test ./...

lint: lint-core lint-mcp

lint-core:
	cd $(CORE_DIR) && $(CARGO) clippy --workspace --all-targets -- -D warnings

lint-mcp:
	cd $(MCP_DIR) && $(GO) vet ./...

fmt:
	cd $(CORE_DIR) && $(CARGO) fmt --all
	$(GOFMT) -w $(MCP_DIR)

fmt-check:
	cd $(CORE_DIR) && $(CARGO) fmt --all --check

check: fmt-check lint test

ci: build fmt-check lint test doc

clean:
	cd $(CORE_DIR) && $(CARGO) clean
	rm -f $(MCP_DIR)/mew-mcp

doc:
	cd $(CORE_DIR) && RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps

help:
	@printf '%s\n' \
		'make build        build everything (Rust + Go MCP)' \
		'make build-core   Rust workspace only' \
		'make build-mcp    Go MCP adapter only' \
		'make run          server + TUI' \
		'make run-server   server only, useful for runtime logs' \
		'make run-tui      TUI only, expects a running server' \
		'make test         all tests' \
		'make lint         clippy + go vet' \
		'make fmt          auto-format' \
		'make check        CI gate (fmt-check + lint + test)' \
		'make ci           full CI gate (build + check + docs)' \
		'make clean        remove build artifacts'
