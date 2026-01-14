package cmd

import "testing"

func TestValidateName(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		wantErr bool
		errMsg  string
	}{
		{
			name:    "valid simple name",
			input:   "feature-login",
			wantErr: false,
		},
		{
			name:    "valid with numbers",
			input:   "bugfix123",
			wantErr: false,
		},
		{
			name:    "valid with dots",
			input:   "v1.0.0",
			wantErr: false,
		},
		{
			name:    "valid with underscores",
			input:   "feature_branch",
			wantErr: false,
		},
		{
			name:    "valid mixed",
			input:   "feature-v1.0_test",
			wantErr: false,
		},
		{
			name:    "valid single char",
			input:   "a",
			wantErr: false,
		},
		{
			name:    "empty name",
			input:   "",
			wantErr: true,
			errMsg:  "cannot be empty",
		},
		{
			name:    "starts with hyphen",
			input:   "-feature",
			wantErr: true,
			errMsg:  "must start with alphanumeric",
		},
		{
			name:    "starts with dot",
			input:   ".hidden",
			wantErr: true,
			errMsg:  "must start with alphanumeric",
		},
		{
			name:    "starts with underscore",
			input:   "_private",
			wantErr: true,
			errMsg:  "must start with alphanumeric",
		},
		{
			name:    "contains space",
			input:   "feature branch",
			wantErr: true,
			errMsg:  "must start with alphanumeric",
		},
		{
			name:    "contains special chars",
			input:   "feature@branch",
			wantErr: true,
			errMsg:  "must start with alphanumeric",
		},
		{
			name:    "reserved name - list",
			input:   "list",
			wantErr: true,
			errMsg:  "reserved name",
		},
		{
			name:    "reserved name - delete",
			input:   "delete",
			wantErr: true,
			errMsg:  "reserved name",
		},
		{
			name:    "reserved name - new",
			input:   "new",
			wantErr: true,
			errMsg:  "reserved name",
		},
		{
			name:    "reserved name - attach",
			input:   "attach",
			wantErr: true,
			errMsg:  "reserved name",
		},
		{
			name:    "reserved name - dot",
			input:   ".",
			wantErr: true,
			errMsg:  "must start with alphanumeric",
		},
		{
			name:    "reserved name - dotdot",
			input:   "..",
			wantErr: true,
			errMsg:  "must start with alphanumeric",
		},
		{
			name:    "too long name",
			input:   "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
			wantErr: true,
			errMsg:  "too long",
		},
		{
			name:    "max length name",
			input:   "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := validateName(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Errorf("validateName(%q) expected error containing %q, got nil", tt.input, tt.errMsg)
				} else if tt.errMsg != "" && !contains(err.Error(), tt.errMsg) {
					t.Errorf("validateName(%q) error = %v, want error containing %q", tt.input, err, tt.errMsg)
				}
			} else {
				if err != nil {
					t.Errorf("validateName(%q) unexpected error: %v", tt.input, err)
				}
			}
		})
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(substr) == 0 ||
		(len(s) > 0 && len(substr) > 0 && searchSubstring(s, substr)))
}

func searchSubstring(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
