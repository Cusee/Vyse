package handlers

import (
	"crypto/subtle"
	"log/slog"
	"net/http"
	"runtime"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/vyse-security/vyse/gateway/internal/config"
	"github.com/vyse-security/vyse/gateway/internal/middleware"
)

// AdminHandler holds dependencies for all admin endpoints.
type AdminHandler struct {
	cfg    *config.Config
	logger *slog.Logger
	// startTime is used to compute gateway uptime in the health endpoint.
	startTime time.Time
}

// NewAdminHandler constructs an AdminHandler.
func NewAdminHandler(cfg *config.Config) *AdminHandler {
	return &AdminHandler{
		cfg:       cfg,
		logger:    slog.Default().With("handler", "admin"),
		startTime: time.Now(),
	}
}

// ── Auth ──────────────────────────────────────────────────────────────────────

// TokenRequest is the JSON body for POST /admin/auth/token.
type TokenRequest struct {
	Username string `json:"username" binding:"required"`
	Password string `json:"password" binding:"required"`
}

// TokenResponse is returned on successful authentication.
type TokenResponse struct {
	Token     string `json:"token"`
	ExpiresIn int    `json:"expires_in_minutes"`
	TokenType string `json:"token_type"`
}

// IssueToken handles POST /admin/auth/token.
//
// Validates username + password and returns a signed JWT.
// Both the username and password comparisons are constant-time to prevent
// timing attacks that could enumerate valid usernames.
func (h *AdminHandler) IssueToken(c *gin.Context) {
	var body TokenRequest
	if err := c.ShouldBindJSON(&body); err != nil {
		c.JSON(http.StatusBadRequest, errorResponse("username and password are required", "BAD_REQUEST"))
		return
	}

	// Constant-time comparison for both fields.
	usernameMatch := subtle.ConstantTimeCompare(
		[]byte(body.Username),
		[]byte(h.cfg.Auth.AdminUsername),
	)
	passwordMatch := subtle.ConstantTimeCompare(
		[]byte(body.Password),
		[]byte(h.cfg.Auth.AdminPassword),
	)

	// Evaluate both comparisons before branching — prevents short-circuit timing leaks.
	if (usernameMatch & passwordMatch) != 1 {
		h.logger.Warn("failed admin login attempt",
			"remote_addr", c.ClientIP(),
			"username", body.Username,
		)
		// Intentionally vague — do not reveal which field was wrong.
		c.JSON(http.StatusUnauthorized, errorResponse("invalid credentials", "UNAUTHORIZED"))
		return
	}

	token, err := middleware.IssueAdminToken(
		body.Username,
		h.cfg.Auth.JWTSecret,
		h.cfg.Auth.JWTExpiryMinutes,
	)
	if err != nil {
		h.logger.Error("failed to sign admin JWT", "error", err)
		c.JSON(http.StatusInternalServerError, errorResponse("token issuance failed", "INTERNAL"))
		return
	}

	h.logger.Info("admin token issued", "username", body.Username, "remote_addr", c.ClientIP())

	c.JSON(http.StatusOK, TokenResponse{
		Token:     token,
		ExpiresIn: h.cfg.Auth.JWTExpiryMinutes,
		TokenType: "Bearer",
	})
}

// ── Health ────────────────────────────────────────────────────────────────────

// HealthResponse is the body returned by GET /health.
type HealthResponse struct {
	Status    string      `json:"status"`
	Service   string      `json:"service"`
	Version   string      `json:"version"`
	UptimeSec float64     `json:"uptime_seconds"`
	Runtime   RuntimeInfo `json:"runtime"`
}

// RuntimeInfo contains Go runtime diagnostics.
type RuntimeInfo struct {
	GoVersion  string `json:"go_version"`
	Goroutines int    `json:"goroutines"`
	GOOS       string `json:"goos"`
	GOARCH     string `json:"goarch"`
}

// Health handles GET /health.
// This endpoint is unauthenticated and is used by load balancers and
// Docker health checks. It must remain fast and never block.
func (h *AdminHandler) Health(c *gin.Context) {
	c.JSON(http.StatusOK, HealthResponse{
		Status:    "healthy",
		Service:   "vyse-gateway",
		Version:   version(),
		UptimeSec: time.Since(h.startTime).Seconds(),
		Runtime: RuntimeInfo{
			GoVersion:  runtime.Version(),
			Goroutines: runtime.NumGoroutine(),
			GOOS:       runtime.GOOS,
			GOARCH:     runtime.GOARCH,
		},
	})
}

// ── Shared helpers ────────────────────────────────────────────────────────────

// errorResponse constructs a consistent error JSON body.
// All error responses across the gateway use this format so clients
// can handle errors uniformly.
func errorResponse(message, code string) gin.H {
	return gin.H{
		"error": message,
		"code":  code,
	}
}

// version returns the current gateway version string.
// In production builds, this is injected at link time with:
//
//	go build -ldflags "-X github.com/vyse-security/vyse/gateway/internal/handlers.buildVersion=1.0.0"
var buildVersion = "dev"

func version() string {
	return buildVersion
}
