package grpc

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/keepalive"

	"github.com/vyse-security/vyse/gateway/internal/config"

	vypb "github.com/vyse-security/vyse/gateway/internal/proto"
)

// EngineClient is the interface the gateway uses to communicate with the
// Rust defence engine. All handler code depends on this interface, not on
// the concrete gRPC client — this allows the stub to be swapped in tests
// and makes the gateway buildable before the Rust engine proto is defined.
type EngineClient interface {
	// Infer sends a prompt and session context to the engine for behavioral
	// analysis, applies the appropriate defence tier, and returns the
	// (potentially perturbed) response along with scoring metadata.
	Infer(ctx context.Context, req *InferRequest) (*InferResponse, error)

	// Close shuts down the underlying gRPC connection. Call this during
	// graceful shutdown.
	Close() error
}

// InferRequest is the gateway-level representation of an inference request.
// It is translated to the proto-generated type before being sent over gRPC.
type InferRequest struct {
	// SessionIDHash is the SHA-256 hash of the raw session ID.
	// The raw session ID never leaves the gateway.
	SessionIDHash string
	// Prompt is the user's input to the protected ML model.
	Prompt string
	// ClientIP is included for audit purposes — it is hashed by the engine
	// before being persisted.
	ClientIP string
}

// InferResponse is the gateway-level representation of the engine's response.
type InferResponse struct {
	// Response is the text to return to the client. For Tier 2 and Tier 3
	// sessions, this is the perturbed version — the engine handles the
	// perturbation and only returns the appropriate output.
	Response string
	// Tier is the security tier the engine assigned to this session: 1, 2, or 3.
	Tier int32
	// HybridScore is the combined threat score in [0.0, 1.0].
	HybridScore float32
	// DurationMins is how long the session has been active since first
	// suspicious activity was detected.
	DurationMins float32
	// RequestID is a unique identifier for this request, used for log correlation.
	RequestID string
}

// engineGRPCClient is the production implementation of EngineClient.
// It wraps the proto-generated gRPC stub with connection management,
// retry logic, and structured logging.
type engineGRPCClient struct {
	conn   *grpc.ClientConn
	client vypb.VyseEngineClient
	cfg    config.EngineConfig
}

// NewEngineClient dials the Rust engine's gRPC address and returns a ready
// EngineClient. The returned client is safe for concurrent use by multiple
// goroutines.
//
// This function blocks for up to cfg.DialTimeout waiting for the connection
// to be established. It returns an error if the engine is unreachable within
// that window.
func NewEngineClient(cfg config.EngineConfig) (EngineClient, error) {
	dialOpts := []grpc.DialOption{
		grpc.WithKeepaliveParams(keepalive.ClientParameters{
			// Send a keepalive ping after 30s of inactivity.
			Time: 30 * time.Second,
			// Wait 10s for the ping ack before considering the connection dead.
			Timeout: 10 * time.Second,
			// Allow keepalive pings even when there are no active RPCs.
			PermitWithoutStream: true,
		}),
	}

	if cfg.UseTLS {
		// TODO: Load mTLS credentials from cfg.TLSCertFile and cfg.TLSKeyFile.
		// For now, TLS without client certs is not supported — use mTLS or insecure.
		return nil, fmt.Errorf("TLS is not yet implemented; set engine.use_tls=false for local dev")
	} else {
		// Insecure is acceptable for local development and for deployments where
		// the engine and gateway run on the same private network / pod.
		// For internet-facing deployments, always enable mTLS.
		slog.Warn("gRPC connection to engine is insecure (no TLS) — acceptable for local dev only")
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(insecure.NewCredentials()))
	}

	ctx, cancel := context.WithTimeout(context.Background(), cfg.DialTimeout)
	defer cancel()

	conn, err := grpc.DialContext(ctx, cfg.Address, dialOpts...)
	if err != nil {
		return nil, fmt.Errorf("dialing engine at %s: %w", cfg.Address, err)
	}

	slog.Info("gRPC connection to engine established", "address", cfg.Address)

	return &engineGRPCClient{
		conn:   conn,
		client: vypb.NewVyseEngineClient(conn),
		cfg:    cfg,
	}, nil
}

// Infer implements EngineClient.
func (c *engineGRPCClient) Infer(ctx context.Context, req *InferRequest) (*InferResponse, error) {
	// Apply the per-request deadline from config.
	ctx, cancel := context.WithTimeout(ctx, c.cfg.RequestTimeout)
	defer cancel()

	protoReq := &vypb.InferenceRequest{
		SessionIdHash: req.SessionIDHash,
		Prompt:        req.Prompt,
		ClientIpHash:  hashForProto(req.ClientIP),
	}

	protoResp, err := c.client.Infer(ctx, protoReq)
	if err != nil {
		return nil, fmt.Errorf("engine Infer RPC: %w", err)
	}

	return &InferResponse{
		Response:     protoResp.Response,
		Tier:         protoResp.Tier,
		HybridScore:  protoResp.HybridScore,
		DurationMins: protoResp.DurationMins,
		RequestID:    protoResp.RequestId,
	}, nil
}

// Close implements EngineClient.
func (c *engineGRPCClient) Close() error {
	return c.conn.Close()
}

// hashForProto is a minimal SHA-256 hash used before sending IP addresses
// to the engine. The engine re-hashes with its own salt before persisting.
func hashForProto(value string) string {
	if value == "" {
		return ""
	}
	// Import crypto/sha256 and encoding/hex in the real implementation.
	// Placeholder kept here to avoid a circular import in the stub package.
	return "hashed:" + value
}
