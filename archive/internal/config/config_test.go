package config

import (
	"os"
	"testing"
	"time"
)

func TestGetEnv(t *testing.T) {
	tests := []struct {
		name       string
		envKey     string
		envValue   string
		defaultVal string
		want       string
	}{
		{
			name:       "returns env value when set",
			envKey:     "TEST_GET_ENV_1",
			envValue:   "custom-value",
			defaultVal: "default",
			want:       "custom-value",
		},
		{
			name:       "returns default when env not set",
			envKey:     "TEST_GET_ENV_2",
			envValue:   "",
			defaultVal: "default",
			want:       "default",
		},
		{
			name:       "returns empty string env value",
			envKey:     "TEST_GET_ENV_3",
			envValue:   "",
			defaultVal: "default",
			want:       "default",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.envValue != "" {
				os.Setenv(tt.envKey, tt.envValue)
				defer os.Unsetenv(tt.envKey)
			}

			got := getEnv(tt.envKey, tt.defaultVal)
			if got != tt.want {
				t.Errorf("getEnv() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestGetEnvInt(t *testing.T) {
	tests := []struct {
		name       string
		envKey     string
		envValue   string
		defaultVal int
		want       int
	}{
		{
			name:       "parses valid int",
			envKey:     "TEST_GET_ENV_INT_1",
			envValue:   "42",
			defaultVal: 10,
			want:       42,
		},
		{
			name:       "returns default on invalid int",
			envKey:     "TEST_GET_ENV_INT_2",
			envValue:   "not-a-number",
			defaultVal: 10,
			want:       10,
		},
		{
			name:       "returns default when not set",
			envKey:     "TEST_GET_ENV_INT_3",
			envValue:   "",
			defaultVal: 10,
			want:       10,
		},
		{
			name:       "parses negative int",
			envKey:     "TEST_GET_ENV_INT_4",
			envValue:   "-5",
			defaultVal: 10,
			want:       -5,
		},
		{
			name:       "parses zero",
			envKey:     "TEST_GET_ENV_INT_5",
			envValue:   "0",
			defaultVal: 10,
			want:       0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.envValue != "" {
				os.Setenv(tt.envKey, tt.envValue)
				defer os.Unsetenv(tt.envKey)
			}

			got := getEnvInt(tt.envKey, tt.defaultVal)
			if got != tt.want {
				t.Errorf("getEnvInt() = %d, want %d", got, tt.want)
			}
		})
	}
}

func TestGetEnvBool(t *testing.T) {
	tests := []struct {
		name       string
		envKey     string
		envValue   string
		defaultVal bool
		want       bool
	}{
		{
			name:       "parses true",
			envKey:     "TEST_GET_ENV_BOOL_1",
			envValue:   "true",
			defaultVal: false,
			want:       true,
		},
		{
			name:       "parses false",
			envKey:     "TEST_GET_ENV_BOOL_2",
			envValue:   "false",
			defaultVal: true,
			want:       false,
		},
		{
			name:       "parses 1 as true",
			envKey:     "TEST_GET_ENV_BOOL_3",
			envValue:   "1",
			defaultVal: false,
			want:       true,
		},
		{
			name:       "parses 0 as false",
			envKey:     "TEST_GET_ENV_BOOL_4",
			envValue:   "0",
			defaultVal: true,
			want:       false,
		},
		{
			name:       "returns default on invalid",
			envKey:     "TEST_GET_ENV_BOOL_5",
			envValue:   "invalid",
			defaultVal: true,
			want:       true,
		},
		{
			name:       "returns default when not set",
			envKey:     "TEST_GET_ENV_BOOL_6",
			envValue:   "",
			defaultVal: true,
			want:       true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.envValue != "" {
				os.Setenv(tt.envKey, tt.envValue)
				defer os.Unsetenv(tt.envKey)
			}

			got := getEnvBool(tt.envKey, tt.defaultVal)
			if got != tt.want {
				t.Errorf("getEnvBool() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestParseDuration(t *testing.T) {
	tests := []struct {
		name       string
		input      string
		defaultVal time.Duration
		want       time.Duration
	}{
		{
			name:       "parses hours",
			input:      "2h",
			defaultVal: time.Hour,
			want:       2 * time.Hour,
		},
		{
			name:       "parses minutes",
			input:      "30m",
			defaultVal: time.Hour,
			want:       30 * time.Minute,
		},
		{
			name:       "parses seconds",
			input:      "45s",
			defaultVal: time.Hour,
			want:       45 * time.Second,
		},
		{
			name:       "parses complex duration",
			input:      "1h30m",
			defaultVal: time.Hour,
			want:       90 * time.Minute,
		},
		{
			name:       "returns default on empty",
			input:      "",
			defaultVal: time.Hour,
			want:       time.Hour,
		},
		{
			name:       "returns default on invalid",
			input:      "invalid",
			defaultVal: time.Hour,
			want:       time.Hour,
		},
		{
			name:       "parses zero duration",
			input:      "0s",
			defaultVal: time.Hour,
			want:       0,
		},
		{
			name:       "parses milliseconds",
			input:      "500ms",
			defaultVal: time.Second,
			want:       500 * time.Millisecond,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := parseDuration(tt.input, tt.defaultVal)
			if got != tt.want {
				t.Errorf("parseDuration() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestEnvoyConfig_GetPollInterval(t *testing.T) {
	tests := []struct {
		name         string
		pollInterval string
		want         time.Duration
	}{
		{
			name:         "parses configured interval",
			pollInterval: "30m",
			want:         30 * time.Minute,
		},
		{
			name:         "returns default on empty",
			pollInterval: "",
			want:         time.Hour,
		},
		{
			name:         "returns default on invalid",
			pollInterval: "invalid",
			want:         time.Hour,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := &EnvoyConfig{
				Sleeves: SleevesConfig{
					PollInterval: tt.pollInterval,
				},
			}
			got := cfg.GetPollInterval()
			if got != tt.want {
				t.Errorf("GetPollInterval() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestEnvoyConfig_GetIdleThreshold(t *testing.T) {
	tests := []struct {
		name          string
		idleThreshold string
		want          time.Duration
	}{
		{
			name:          "parses configured threshold",
			idleThreshold: "2h",
			want:          2 * time.Hour,
		},
		{
			name:          "returns zero on empty",
			idleThreshold: "",
			want:          0,
		},
		{
			name:          "returns zero on invalid",
			idleThreshold: "invalid",
			want:          0,
		},
		{
			name:          "parses 0s as zero",
			idleThreshold: "0s",
			want:          0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := &EnvoyConfig{
				Sleeves: SleevesConfig{
					IdleThreshold: tt.idleThreshold,
				},
			}
			got := cfg.GetIdleThreshold()
			if got != tt.want {
				t.Errorf("GetIdleThreshold() = %v, want %v", got, tt.want)
			}
		})
	}
}
