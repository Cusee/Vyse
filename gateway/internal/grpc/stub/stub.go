// Package stub provides placeholder types that mirror what the proto-generated
// gRPC package will export once proto/vyse.proto is defined and compiled.
//
// PURPOSE:
//
//	This stub allows the gateway to compile and run in local development
//	before the Rust engine's proto definition exists. It returns hardcoded
//	Tier 1 responses so the full HTTP stack can be exercised end-to-end
//	without a running engine.
//
// REPLACING THIS STUB:
//
//	Once proto/vyse.proto is defined:
//	1. Run: make proto-gen
//	2. The generated package appears at internal/proto/
//	3. In internal/grpc/client.go, change the import from:
//	     vypb "github.com/vyse-security/vyse/gateway/internal/grpc/stub"
//	   to:
//	     vypb "github.com/vyse-security/vyse/gateway/internal/proto"
//	4. Delete this stub package.
//
// DO NOT use this package in production.
package stub

import (
	"context"
	"log/slog"

	"google.golang.org/grpc"
)

// ── Proto message types ───────────────────────────────────────────────────────
// These mirror the fields that will be generated from proto/vyse.proto.
// Field names must stay in sync with the proto definition when it is written.

// InferenceRequest mirrors the proto InferenceRequest message.
type InferenceRequest struct {
	SessionIdHash string
	Prompt        string
	ClientIpHash  string
}

// InferenceResponse mirrors the proto InferenceResponse message.
type InferenceResponse struct {
	Response     string
	Tier         int32
	HybridScore  float32
	DurationMins float32
	RequestId    string
}

// ── gRPC client interface ─────────────────────────────────────────────────────

// VyseEngineClient is the interface the stub implements.
// The real proto-generated client will implement the same interface.
type VyseEngineClient interface {
	Infer(ctx context.Context, req *InferenceRequest, opts ...grpc.CallOption) (*InferenceResponse, error)
}

// NewVyseEngineClient returns a stub client that never contacts a real engine.
func NewVyseEngineClient(_ grpc.ClientConnInterface) VyseEngineClient {
	slog.Warn("⚠️  using stub gRPC engine client — responses are mocked. Replace with real proto client.")
	return &stubClient{}
}

// stubClient is the no-op implementation.
type stubClient struct{}

// Infer always returns a Tier 1 stub response.
// It logs the prompt so you can verify the full request path is working.
func (s *stubClient) Infer(_ context.Context, req *InferenceRequest, _ ...grpc.CallOption) (*InferenceResponse, error) {
	slog.Debug("stub engine: received Infer call",
		"session_hash", req.SessionIdHash[:min(8, len(req.SessionIdHash))],
		"prompt_len", len(req.Prompt),
	)

	return &InferenceResponse{
		Response:     "[STUB] Engine not connected. This is a mock response for local gateway development.",
		Tier:         1,
		HybridScore:  0.0,
		DurationMins: 0.0,
		RequestId:    "stub-request-id",
	}, nil
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
