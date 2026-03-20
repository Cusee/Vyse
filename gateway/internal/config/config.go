package config

import (
	"fmt"
	"strings"
	"time"

	"github.com/spf13/viper"
)

// Config is the single source of truth for all gateway configuration.
// Values are loaded from config.toml first; environment variables with
// the VYSE_ prefix override any file value.
//
// Example: VYSE_SERVER_PORT=9090 overrides server.port in config.toml.
type Config struct {
	Server  ServerConfig  `mapstructure:"server"`
	Auth    AuthConfig    `mapstructure:"auth"`
	Engine  EngineConfig  `mapstructure:"engine"`
	Limits  LimitsConfig  `mapstructure:"limits"`
	Logging LoggingConfig `mapstructure:"logging"`
}

// ServerConfig controls the HTTP listener.
type ServerConfig struct {
	// PublicPort is the port that accepts inference requests from clients.
	PublicPort int `mapstructure:"public_port"`
	// AdminPort is the port that accepts JWT-protected admin requests.
	AdminPort int `mapstructure:"admin_port"`
	// ReadTimeout is the maximum duration for reading the full request.
	ReadTimeout time.Duration `mapstructure:"read_timeout"`
	// WriteTimeout is the maximum duration for writing the full response.
	WriteTimeout time.Duration `mapstructure:"write_timeout"`
	// ShutdownTimeout is the maximum duration for graceful shutdown.
	ShutdownTimeout time.Duration `mapstructure:"shutdown_timeout"`
	// TrustedProxies is the list of upstream proxy IPs (e.g. load balancer).
	// Set to [] to trust no proxies. Set to nil to trust all (not recommended).
	TrustedProxies []string `mapstructure:"trusted_proxies"`
}

// AuthConfig controls JWT and API key authentication.
type AuthConfig struct {
	// JWTSecret is the HMAC-SHA256 signing key for admin JWTs.
	// Must be set via VYSE_AUTH_JWT_SECRET environment variable.
	// Never put this value in config.toml.
	JWTSecret string `mapstructure:"jwt_secret"`
	// JWTExpiryMinutes is the TTL for issued admin tokens.
	JWTExpiryMinutes int `mapstructure:"jwt_expiry_minutes"`
	// AdminUsername is the username accepted at POST /admin/auth/token.
	AdminUsername string `mapstructure:"admin_username"`
	// AdminPassword is the bcrypt hash of the admin password.
	// Must be set via VYSE_AUTH_ADMIN_PASSWORD environment variable.
	// Never put this value in config.toml.
	AdminPassword string `mapstructure:"admin_password"`
	// APIKeyHeader is the header name that clients use to send the API key.
	APIKeyHeader string `mapstructure:"api_key_header"`
	// APIKey is the shared secret that inference clients must present.
	// Must be set via VYSE_AUTH_API_KEY environment variable.
	APIKey string `mapstructure:"api_key"`
}

// EngineConfig controls the gRPC connection to the Rust defence engine.
type EngineConfig struct {
	// Address is host:port of the Rust engine's gRPC listener.
	Address string `mapstructure:"address"`
	// DialTimeout is the maximum time to wait when establishing the gRPC connection.
	DialTimeout time.Duration `mapstructure:"dial_timeout"`
	// RequestTimeout is the per-request deadline sent to the engine.
	RequestTimeout time.Duration `mapstructure:"request_timeout"`
	// MaxRetries is the number of times to retry a failed gRPC call before giving up.
	MaxRetries int `mapstructure:"max_retries"`
	// UseTLS enables mTLS for the gRPC connection. Requires TLSCertFile and TLSKeyFile.
	UseTLS bool `mapstructure:"use_tls"`
	// TLSCertFile is the path to the client TLS certificate (mTLS).
	TLSCertFile string `mapstructure:"tls_cert_file"`
	// TLSKeyFile is the path to the client TLS private key (mTLS).
	TLSKeyFile string `mapstructure:"tls_key_file"`
}

// LimitsConfig controls rate limiting applied at the gateway level.
// These limits are a first line of defence and complement the engine's
// behavioral scoring. They operate at the IP level, not the session level.
type LimitsConfig struct {
	// RequestsPerSecond is the token bucket refill rate per IP address.
	RequestsPerSecond float64 `mapstructure:"requests_per_second"`
	// Burst is the maximum number of requests an IP can make in a single burst.
	Burst int `mapstructure:"burst"`
	// MaxPromptBytes is the maximum allowed size of the prompt field in bytes.
	// Requests exceeding this are rejected with HTTP 413 before reaching the engine.
	MaxPromptBytes int `mapstructure:"max_prompt_bytes"`
	// SessionHeaderName is the header clients use to identify their session.
	SessionHeaderName string `mapstructure:"session_header_name"`
}

// LoggingConfig controls structured log output.
type LoggingConfig struct {
	// Level is the minimum log level: "debug", "info", "warn", "error".
	Level string `mapstructure:"level"`
	// Format is the output format: "json" for production, "text" for development.
	Format string `mapstructure:"format"`
}

// Load reads configuration from config.toml (if present) and then applies
// environment variable overrides. The config file path defaults to ./config.toml
// but can be overridden with VYSE_CONFIG_FILE.
//
// Secrets (JWTSecret, AdminPassword, APIKey) must be provided via environment
// variables — Load returns an error if any are empty after loading.
func Load() (*Config, error) {
	v := viper.New()

	// ── Defaults ──────────────────────────────────────────────────────────────
	v.SetDefault("server.public_port", 8080)
	v.SetDefault("server.admin_port", 8081)
	v.SetDefault("server.read_timeout", "10s")
	v.SetDefault("server.write_timeout", "30s")
	v.SetDefault("server.shutdown_timeout", "15s")
	v.SetDefault("server.trusted_proxies", []string{})

	v.SetDefault("auth.jwt_expiry_minutes", 480)
	v.SetDefault("auth.admin_username", "admin")
	v.SetDefault("auth.api_key_header", "X-Vyse-Key")

	v.SetDefault("engine.address", "localhost:50051")
	v.SetDefault("engine.dial_timeout", "5s")
	v.SetDefault("engine.request_timeout", "30s")
	v.SetDefault("engine.max_retries", 3)
	v.SetDefault("engine.use_tls", false)

	v.SetDefault("limits.requests_per_second", 10.0)
	v.SetDefault("limits.burst", 20)
	v.SetDefault("limits.max_prompt_bytes", 16384) // 16 KB
	v.SetDefault("limits.session_header_name", "X-Vyse-Session")

	v.SetDefault("logging.level", "info")
	v.SetDefault("logging.format", "json")

	// ── Config file ───────────────────────────────────────────────────────────
	v.SetConfigName("config")
	v.SetConfigType("toml")
	v.AddConfigPath(".")
	v.AddConfigPath("/etc/vyse/gateway/")

	if err := v.ReadInConfig(); err != nil {
		// A missing config file is acceptable — env vars are sufficient.
		if _, ok := err.(viper.ConfigFileNotFoundError); !ok {
			return nil, fmt.Errorf("reading config file: %w", err)
		}
	}

	// ── Environment variable overrides ────────────────────────────────────────
	// All env vars are prefixed VYSE_ and use underscores as path separators.
	// Example: VYSE_SERVER_PUBLIC_PORT=9090 overrides server.public_port.
	v.SetEnvPrefix("VYSE")
	v.SetEnvKeyReplacer(strings.NewReplacer(".", "_"))
	v.AutomaticEnv()

	var cfg Config
	if err := v.Unmarshal(&cfg); err != nil {
		return nil, fmt.Errorf("unmarshalling config: %w", err)
	}

	if err := cfg.validate(); err != nil {
		return nil, fmt.Errorf("invalid config: %w", err)
	}

	return &cfg, nil
}

// validate checks that all required secrets are present and that
// numeric values are within acceptable ranges.
func (c *Config) validate() error {
	var errs []string

	if c.Auth.JWTSecret == "" {
		errs = append(errs, "auth.jwt_secret must be set via VYSE_AUTH_JWT_SECRET")
	}
	if len(c.Auth.JWTSecret) < 32 {
		errs = append(errs, "auth.jwt_secret must be at least 32 characters")
	}
	if c.Auth.AdminPassword == "" {
		errs = append(errs, "auth.admin_password must be set via VYSE_AUTH_ADMIN_PASSWORD")
	}
	if c.Auth.APIKey == "" {
		errs = append(errs, "auth.api_key must be set via VYSE_AUTH_API_KEY")
	}
	if len(c.Auth.APIKey) < 32 {
		errs = append(errs, "auth.api_key must be at least 32 characters")
	}
	if c.Engine.Address == "" {
		errs = append(errs, "engine.address must not be empty")
	}
	if c.Limits.RequestsPerSecond <= 0 {
		errs = append(errs, "limits.requests_per_second must be greater than 0")
	}
	if c.Limits.Burst <= 0 {
		errs = append(errs, "limits.burst must be greater than 0")
	}
	if c.Limits.MaxPromptBytes <= 0 {
		errs = append(errs, "limits.max_prompt_bytes must be greater than 0")
	}

	if len(errs) > 0 {
		return fmt.Errorf("\n  - %s", strings.Join(errs, "\n  - "))
	}
	return nil
}
