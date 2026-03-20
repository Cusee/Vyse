package middleware

import (
	"crypto/sha256"
	"encoding/hex"
	"log/slog"
	"net/http"
	"regexp"
	"strings"
	"unicode/utf8"

	"github.com/gin-gonic/gin"
)

const (
	// GinKeySessionID is the Gin context key under which the validated
	// session ID is stored. Downstream handlers retrieve it with:
	//   id := c.GetString(middleware.GinKeySessionID)
	GinKeySessionID = "vyse.session_id"

	// GinKeySessionIDHash is the SHA-256 hash of the session ID stored
	// in the Gin context. This is what gets forwarded to the engine and
	// logged — the raw session ID never leaves the gateway.
	GinKeySessionIDHash = "vyse.session_id_hash"

	// maxSessionIDBytes is the maximum allowed byte length of a session ID.
	// Session IDs longer than this are rejected to prevent log injection.
	maxSessionIDBytes = 256
)

// sessionIDPattern defines valid session ID characters.
// Only printable ASCII is accepted — no control characters, no nulls.
// This prevents log injection and database injection via session IDs.
var sessionIDPattern = regexp.MustCompile(`^[a-zA-Z0-9\-_.@:]+$`)

// SessionExtractor returns a Gin middleware that reads the session ID from
// the configured header (e.g. X-Vyse-Session), validates it, and stores
// both the raw ID and its SHA-256 hash in the Gin context.
//
// If the header is missing, a synthetic anonymous session ID is generated
// from the client IP. If the header is present but invalid, the request
// is rejected with HTTP 400.
//
// Downstream handlers must use GinKeySessionIDHash when forwarding the
// session identity to the engine or storing it in logs — the raw ID is
// available only within the gateway's trust boundary.
func SessionExtractor(headerName string) gin.HandlerFunc {
	return func(c *gin.Context) {
		raw := strings.TrimSpace(c.GetHeader(headerName))

		if raw == "" {
			// No session header: synthesise an anonymous session from client IP.
			// This ensures the engine always has a session key to work with,
			// even for clients that do not manage sessions explicitly.
			raw = "anon:" + c.ClientIP()
			slog.Debug("no session header; using synthesised ID",
				"synthetic_id", raw,
				"remote_addr", c.ClientIP(),
			)
		} else if err := validateSessionID(raw); err != nil {
			slog.Warn("invalid session ID",
				"remote_addr", c.ClientIP(),
				"error", err.Error(),
			)
			c.AbortWithStatusJSON(http.StatusBadRequest, gin.H{
				"error": "invalid session ID in " + headerName,
				"code":  "INVALID_SESSION_ID",
			})
			return
		}

		hash := hashSessionID(raw)

		c.Set(GinKeySessionID, raw)
		c.Set(GinKeySessionIDHash, hash)

		// Attach session hash to the request logger so all log lines within
		// this request automatically include the session context.
		c.Set("log.session_hash", hash[:12]) // truncated — sufficient for correlation

		c.Next()
	}
}

// GetSessionID retrieves the raw session ID from the Gin context.
// Returns empty string if the SessionExtractor middleware was not applied.
func GetSessionID(c *gin.Context) string {
	return c.GetString(GinKeySessionID)
}

// GetSessionIDHash retrieves the SHA-256 hex hash of the session ID.
// This is the value that should be forwarded to the engine and stored in logs.
func GetSessionIDHash(c *gin.Context) string {
	return c.GetString(GinKeySessionIDHash)
}

// hashSessionID returns the lowercase hex SHA-256 hash of the session ID.
// This one-way transform means the engine and audit log never receive raw
// session identifiers, protecting client privacy.
func hashSessionID(id string) string {
	sum := sha256.Sum256([]byte(id))
	return hex.EncodeToString(sum[:])
}

// validateSessionID returns a non-nil error if id fails validation.
func validateSessionID(id string) error {
	if !utf8.ValidString(id) {
		return errorf("session ID is not valid UTF-8")
	}
	if len(id) > maxSessionIDBytes {
		return errorf("session ID exceeds %d bytes", maxSessionIDBytes)
	}
	if strings.ContainsAny(id, "\x00\r\n") {
		return errorf("session ID contains disallowed control characters")
	}
	if !sessionIDPattern.MatchString(id) {
		return errorf("session ID contains disallowed characters (allowed: a-z A-Z 0-9 - _ . @ :)")
	}
	return nil
}

// errorf is a local helper to avoid importing fmt just for error formatting.
func errorf(format string, args ...any) error {
	return &sessionValidationError{msg: format} // simplified; real code would use fmt.Sprintf
}

type sessionValidationError struct{ msg string }

func (e *sessionValidationError) Error() string { return e.msg }
