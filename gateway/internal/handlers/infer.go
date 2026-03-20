package handlers

import (
	"log/slog"
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"

	"github.com/vyse-security/vyse/gateway/internal/config"
	vygrpc "github.com/vyse-security/vyse/gateway/internal/grpc"
	"github.com/vyse-security/vyse/gateway/internal/middleware"
)

// InferHandler holds the dependencies for the inference endpoint.
// Constructed once at startup and reused across all requests.
type InferHandler struct {
	engine    vygrpc.EngineClient
	maxPrompt int
	logger    *slog.Logger
}

// InferRequest is the JSON body accepted by POST /api/infer.
type InferRequest struct {
	// Prompt is the user's input to the protected ML model.
	// Required. Maximum length is controlled by limits.max_prompt_bytes in config.
	Prompt string `json:"prompt" binding:"required"`
}

// InferResponse is the JSON body returned by POST /api/infer.
type InferResponse struct {
	// Response is the model output. For Tier 2 and Tier 3 sessions, this is
	// the perturbed version — the engine decides what to serve.
	Response string `json:"response"`
	// Tier is the security tier the engine assigned to this session.
	// Clients receive this for transparency; it should not be used to infer
	// whether their session is under scrutiny (the value is always present).
	Tier int32 `json:"tier"`
	// RequestID is an opaque identifier for this request, useful for
	// correlating client-side logs with gateway and engine logs.
	RequestID string `json:"request_id"`
}

// NewInferHandler constructs an InferHandler.
func NewInferHandler(engine vygrpc.EngineClient, cfg *config.Config) *InferHandler {
	return &InferHandler{
		engine:    engine,
		maxPrompt: cfg.Limits.MaxPromptBytes,
		logger:    slog.Default().With("handler", "infer"),
	}
}

// Handle handles POST /api/infer.
//
// This is the hot path — every client inference request goes through here.
// The handler is intentionally thin: validate input, forward to engine, return output.
// All security logic lives in the engine.
func (h *InferHandler) Handle(c *gin.Context) {
	sessionHash := middleware.GetSessionIDHash(c)
	if sessionHash == "" {
		// Session middleware was not applied — this is a misconfigured router.
		h.logger.Error("session hash missing from context — check router setup")
		c.JSON(http.StatusInternalServerError, errorResponse("internal configuration error", "INTERNAL"))
		return
	}

	var body InferRequest
	if err := c.ShouldBindJSON(&body); err != nil {
		c.JSON(http.StatusBadRequest, errorResponse("request body must be valid JSON with a non-empty 'prompt' field", "BAD_REQUEST"))
		return
	}

	// Enforce prompt size limit before forwarding to the engine.
	// This is a gateway-level guard — the engine has its own limits too.
	if len(body.Prompt) > h.maxPrompt {
		c.JSON(http.StatusRequestEntityTooLarge, errorResponse(
			"prompt exceeds maximum allowed size",
			"PROMPT_TOO_LARGE",
		))
		return
	}

	// Reject prompts that are pure whitespace — they provide no signal
	// to the engine and waste inference compute.
	if strings.TrimSpace(body.Prompt) == "" {
		c.JSON(http.StatusBadRequest, errorResponse("prompt must not be empty or whitespace", "BAD_REQUEST"))
		return
	}

	engineReq := &vygrpc.InferRequest{
		SessionIDHash: sessionHash,
		Prompt:        body.Prompt,
		ClientIP:      c.ClientIP(),
	}

	h.logger.Debug("forwarding to engine",
		"session_hash_prefix", sessionHash[:12],
		"prompt_len", len(body.Prompt),
	)

	resp, err := h.engine.Infer(c.Request.Context(), engineReq)
	if err != nil {
		h.logger.Error("engine Infer call failed",
			"session_hash_prefix", sessionHash[:12],
			"error", err,
		)
		// Return a generic error — do not leak internal error details to the client.
		c.JSON(http.StatusServiceUnavailable, errorResponse("inference service unavailable", "ENGINE_UNAVAILABLE"))
		return
	}

	h.logger.Info("inference complete",
		"session_hash_prefix", sessionHash[:12],
		"tier", resp.Tier,
		"hybrid_score", resp.HybridScore,
		"request_id", resp.RequestID,
	)

	c.JSON(http.StatusOK, InferResponse{
		Response:  resp.Response,
		Tier:      resp.Tier,
		RequestID: resp.RequestID,
	})
}
