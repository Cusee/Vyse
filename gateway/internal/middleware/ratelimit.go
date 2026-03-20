package middleware

import (
	"log/slog"
	"net/http"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// ipLimiter holds the token bucket limiter and the last-seen time for a
// single IP address. The last-seen time is used for eviction of idle entries.
type ipLimiter struct {
	limiter  *rate.Limiter
	lastSeen time.Time
}

// RateLimiter is a per-IP token bucket rate limiter backed by an in-memory map.
// It is safe for concurrent use.
//
// Design notes:
//   - One limiter instance per unique client IP address.
//   - Stale entries (IPs not seen for > cleanupInterval) are evicted to prevent
//     unbounded memory growth under a distributed source attack.
//   - The map is protected by a sync.RWMutex — reads (check) use RLock,
//     writes (add new IP) use Lock.
type RateLimiter struct {
	mu              sync.RWMutex
	limiters        map[string]*ipLimiter
	rps             rate.Limit // tokens refilled per second
	burst           int        // maximum token bucket depth
	cleanupInterval time.Duration
	idleTimeout     time.Duration
}

// NewRateLimiter creates a RateLimiter and starts the background cleanup goroutine.
//
// rps is the sustained requests-per-second allowed per IP.
// burst is the maximum number of requests an IP can make in a single burst.
func NewRateLimiter(rps float64, burst int) *RateLimiter {
	rl := &RateLimiter{
		limiters:        make(map[string]*ipLimiter),
		rps:             rate.Limit(rps),
		burst:           burst,
		cleanupInterval: 5 * time.Minute,
		idleTimeout:     10 * time.Minute,
	}

	go rl.cleanupLoop()
	return rl
}

// Middleware returns a Gin handler that enforces the rate limit.
// Requests that exceed the limit receive HTTP 429 with a Retry-After header.
func (rl *RateLimiter) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		ip := c.ClientIP()
		limiter := rl.getLimiter(ip)

		if !limiter.Allow() {
			slog.Warn("rate limit exceeded",
				"remote_addr", ip,
				"path", c.Request.URL.Path,
				"method", c.Request.Method,
			)
			c.Header("Retry-After", "1")
			c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
				"error": "rate limit exceeded — slow down",
				"code":  "RATE_LIMITED",
			})
			return
		}

		c.Next()
	}
}

// getLimiter returns the existing limiter for ip, creating one if needed.
func (rl *RateLimiter) getLimiter(ip string) *rate.Limiter {
	// Fast path: limiter already exists.
	rl.mu.RLock()
	if entry, ok := rl.limiters[ip]; ok {
		entry.lastSeen = time.Now()
		l := entry.limiter
		rl.mu.RUnlock()
		return l
	}
	rl.mu.RUnlock()

	// Slow path: create a new limiter for this IP.
	rl.mu.Lock()
	defer rl.mu.Unlock()

	// Double-check after acquiring the write lock — another goroutine may have
	// created the entry between our RUnlock and Lock.
	if entry, ok := rl.limiters[ip]; ok {
		entry.lastSeen = time.Now()
		return entry.limiter
	}

	l := rate.NewLimiter(rl.rps, rl.burst)
	rl.limiters[ip] = &ipLimiter{
		limiter:  l,
		lastSeen: time.Now(),
	}
	return l
}

// cleanupLoop runs in a goroutine and evicts IP limiters that have not made
// a request within idleTimeout. This prevents the map from growing without
// bound when many unique IPs are seen (e.g. during a distributed attack).
func (rl *RateLimiter) cleanupLoop() {
	ticker := time.NewTicker(rl.cleanupInterval)
	defer ticker.Stop()

	for range ticker.C {
		rl.evictIdle()
	}
}

func (rl *RateLimiter) evictIdle() {
	cutoff := time.Now().Add(-rl.idleTimeout)

	rl.mu.Lock()
	defer rl.mu.Unlock()

	evicted := 0
	for ip, entry := range rl.limiters {
		if entry.lastSeen.Before(cutoff) {
			delete(rl.limiters, ip)
			evicted++
		}
	}

	if evicted > 0 {
		slog.Debug("evicted idle IP rate limiters",
			"count", evicted,
			"remaining", len(rl.limiters),
		)
	}
}
