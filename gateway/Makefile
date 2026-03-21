# Vyse Gateway — Makefile
# ─────────────────────────────────────────────────────────────────────────────
# Usage:
#   make build          build the gateway binary
#   make run            run with config.toml (requires .env to be sourced)
#   make test           run all tests
#   make test-race      run tests with the race detector
#   make lint           run golangci-lint
#   make fmt            format all Go source
#   make proto-gen      regenerate gRPC stubs from proto/vyse.proto
#   make clean          remove build artifacts
#   make help           show this message

BINARY     := vyse-gateway
BUILD_DIR  := ./bin
MAIN       := ./cmd/vyse-gateway
VERSION    ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
LDFLAGS    := -X github.com/vyse-security/vyse/gateway/internal/handlers.buildVersion=$(VERSION)
GOFLAGS    := -trimpath

# Protobuf settings
# Update PROTO_DIR to point at the monorepo proto/ directory once it exists.
PROTO_DIR  := ../proto
PROTO_OUT_GO := ./internal/proto

.PHONY: all build run test test-race test-cover lint fmt vet proto-gen clean help

all: lint test build

# ── Build ─────────────────────────────────────────────────────────────────────

build:
	@echo "→ building $(BINARY) $(VERSION)"
	@mkdir -p $(BUILD_DIR)
	go build $(GOFLAGS) -ldflags "$(LDFLAGS)" -o $(BUILD_DIR)/$(BINARY) $(MAIN)
	@echo "✓ $(BUILD_DIR)/$(BINARY)"

build-race:
	@echo "→ building $(BINARY) with race detector"
	@mkdir -p $(BUILD_DIR)
	go build -race $(GOFLAGS) -ldflags "$(LDFLAGS)" -o $(BUILD_DIR)/$(BINARY)-race $(MAIN)

run:
	@echo "→ running gateway (dev mode)"
	go run $(MAIN) --config config.toml

# ── Tests ─────────────────────────────────────────────────────────────────────

test:
	@echo "→ running tests"
	go test ./... -count=1 -timeout 60s

# Always run with the race detector before opening a PR.
# The race detector catches concurrent map writes, data races on session state,
# and other concurrency bugs that only manifest under load.
test-race:
	@echo "→ running tests with race detector"
	go test -race ./... -count=1 -timeout 120s

test-cover:
	@echo "→ running tests with coverage"
	go test ./... -count=1 -timeout 60s -coverprofile=coverage.out -covermode=atomic
	go tool cover -html=coverage.out -o coverage.html
	@echo "✓ coverage report: coverage.html"

# ── Quality ───────────────────────────────────────────────────────────────────

lint:
	@echo "→ running golangci-lint"
	@which golangci-lint > /dev/null || (echo "install: go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest" && exit 1)
	golangci-lint run ./...

fmt:
	@echo "→ formatting"
	gofmt -w .
	@echo "✓ gofmt done"

vet:
	@echo "→ running go vet"
	go vet ./...

# ── Protobuf ──────────────────────────────────────────────────────────────────

# Regenerate Go gRPC stubs from proto/vyse.proto.
# Requires: protoc, protoc-gen-go, protoc-gen-go-grpc
#
# Install generators:
#   go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
#   go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
#
# After running this target:
#   1. Generated files appear in internal/proto/
#   2. Update the import in internal/grpc/client.go (see comment in that file)
#   3. Delete internal/grpc/stub/ once the real proto is wired
#   4. Commit both the .proto change and the generated files in the same commit
proto-gen:
	@echo "→ generating Go gRPC stubs from $(PROTO_DIR)/vyse.proto"
	@which protoc > /dev/null || (echo "protoc not found — install from https://github.com/protocolbuffers/protobuf/releases" && exit 1)
	@mkdir -p $(PROTO_OUT_GO)
	protoc \
		--proto_path=$(PROTO_DIR) \
		--go_out=$(PROTO_OUT_GO) \
		--go_opt=paths=source_relative \
		--go-grpc_out=$(PROTO_OUT_GO) \
		--go-grpc_opt=paths=source_relative \
		$(PROTO_DIR)/vyse.proto
	@echo "✓ stubs generated in $(PROTO_OUT_GO)"
	@echo "  → update import in internal/grpc/client.go and delete internal/grpc/stub/"

# ── Misc ──────────────────────────────────────────────────────────────────────

clean:
	@echo "→ cleaning"
	rm -rf $(BUILD_DIR) coverage.out coverage.html
	@echo "✓ clean"

help:
	@echo ""
	@echo "Vyse Gateway — available targets:"
	@echo ""
	@echo "  build        build the binary to ./bin/$(BINARY)"
	@echo "  run          run in development mode"
	@echo "  test         run all tests"
	@echo "  test-race    run tests with race detector (run before every PR)"
	@echo "  test-cover   run tests and generate HTML coverage report"
	@echo "  lint         run golangci-lint"
	@echo "  fmt          format all Go source with gofmt"
	@echo "  vet          run go vet"
	@echo "  proto-gen    regenerate gRPC stubs from proto/vyse.proto"
	@echo "  clean        remove build artifacts"
	@echo ""