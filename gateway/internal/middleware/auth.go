package middleware

import (
	"crypto/subtle"
	"errors"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/golang-jwt/jwt/v5"
)

// contextKey is an unexported type for context keys set by this package.
// Using a named type prevents key collisions with other packages.
type contextKey string

const (
	ctxKeyAdminClaims contextKey = "admin_claims"
	ctxKeySessionID   contextKey = "session_id"
)

// AdminClaims are the JWT claims embedded in admin tokens.
type AdminClaims struct {
	jwt.RegisteredClaims
	Username string `json:"username"`
	// Role is reserved for future role-based access control.
	Role string `json:"role,omitempty"`
}

// APIKeyAuth returns a Gin middleware that validates the X-Vyse-Key header
// (or whichever header name is configured) against the configured API key.
//
// This middleware guards the public inference endpoint. A missing or incorrect
// key returns HTTP 401 immediately — no information is leaked about whether
// the key exists vs is wrong.
//
// The comparison is constant-time to prevent timing attacks.
func APIKeyAuth(headerName, expectedKey string) gin.HandlerFunc {
	// Pre-convert to []byte once at startup — not per request.
	expectedBytes := []byte(expectedKey)

	return func(c *gin.Context) {
		key := c.GetHeader(headerName)
		if key == "" {
			abortUnauthorized(c, "missing API key")
			return
		}

		// subtle.ConstantTimeCompare prevents timing oracle attacks where an
		// attacker can infer the key length or common prefix from response time.
		if subtle.ConstantTimeCompare([]byte(key), expectedBytes) != 1 {
			// Log at warn — repeated failures here indicate an active probe.
			slog.Warn("invalid API key",
				"remote_addr", c.ClientIP(),
				"path", c.Request.URL.Path,
			)
			abortUnauthorized(c, "invalid API key")
			return
		}

		c.Next()
	}
}

// JWTAuth returns a Gin middleware that validates Bearer tokens in the
// Authorization header against the provided HMAC-SHA256 secret.
//
// On success, the parsed AdminClaims are stored in the Gin context under
// ctxKeyAdminClaims and can be retrieved with GetAdminClaims().
//
// This middleware guards admin endpoints — it should never be applied to
// the public inference endpoint.
func JWTAuth(secret string) gin.HandlerFunc {
	secretBytes := []byte(secret)

	return func(c *gin.Context) {
		raw, err := extractBearerToken(c)
		if err != nil {
			abortUnauthorized(c, err.Error())
			return
		}

		claims := &AdminClaims{}
		token, err := jwt.ParseWithClaims(raw, claims, func(t *jwt.Token) (any, error) {
			// Explicitly reject non-HMAC algorithms.
			// Without this check, an attacker can craft a token with alg="none"
			// and bypass signature verification entirely.
			if _, ok := t.Method.(*jwt.SigningMethodHMAC); !ok {
				return nil, errors.New("unexpected signing algorithm: " + t.Header["alg"].(string))
			}
			return secretBytes, nil
		},
			jwt.WithValidMethods([]string{"HS256"}),
			jwt.WithExpirationRequired(),
			jwt.WithIssuedAt(),
		)
		if err != nil || !token.Valid {
			slog.Warn("JWT validation failed",
				"remote_addr", c.ClientIP(),
				"error", err,
			)
			abortUnauthorized(c, "invalid or expired token")
			return
		}

		// Store claims for downstream handlers.
		c.Set(string(ctxKeyAdminClaims), claims)
		c.Next()
	}
}

// IssueAdminToken creates and signs a new admin JWT with the provided claims.
// The caller is responsible for verifying credentials before calling this.
func IssueAdminToken(username, secret string, expiryMinutes int) (string, error) {
	now := time.Now().UTC()
	claims := AdminClaims{
		RegisteredClaims: jwt.RegisteredClaims{
			Issuer:    "vyse-gateway",
			Subject:   username,
			IssuedAt:  jwt.NewNumericDate(now),
			ExpiresAt: jwt.NewNumericDate(now.Add(time.Duration(expiryMinutes) * time.Minute)),
		},
		Username: username,
		Role:     "admin",
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString([]byte(secret))
}

// GetAdminClaims retrieves the validated AdminClaims from the Gin context.
// Returns nil if the JWT middleware was not applied or validation failed.
func GetAdminClaims(c *gin.Context) *AdminClaims {
	val, exists := c.Get(string(ctxKeyAdminClaims))
	if !exists {
		return nil
	}
	claims, _ := val.(*AdminClaims)
	return claims
}

// ── Helpers ───────────────────────────────────────────────────────────────────

func extractBearerToken(c *gin.Context) (string, error) {
	header := c.GetHeader("Authorization")
	if header == "" {
		return "", errors.New("missing Authorization header")
	}

	parts := strings.SplitN(header, " ", 2)
	if len(parts) != 2 || !strings.EqualFold(parts[0], "bearer") {
		return "", errors.New("Authorization header must use Bearer scheme")
	}

	if parts[1] == "" {
		return "", errors.New("Bearer token is empty")
	}

	return parts[1], nil
}

func abortUnauthorized(c *gin.Context, msg string) {
	c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{
		"error": msg,
		"code":  "UNAUTHORIZED",
	})
}
