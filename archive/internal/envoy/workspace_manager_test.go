package envoy

import "testing"

func TestRepoNameFromURL(t *testing.T) {
	tests := []struct {
		name string
		url  string
		want string
	}{
		{
			name: "HTTPS URL with .git suffix",
			url:  "https://github.com/user/repo.git",
			want: "repo",
		},
		{
			name: "HTTPS URL without .git suffix",
			url:  "https://github.com/user/repo",
			want: "repo",
		},
		{
			name: "HTTPS URL with trailing slash",
			url:  "https://github.com/user/repo/",
			want: "repo",
		},
		{
			name: "SSH URL",
			url:  "git@github.com:user/repo.git",
			want: "repo",
		},
		{
			name: "GitLab HTTPS URL",
			url:  "https://gitlab.com/group/subgroup/repo.git",
			want: "repo",
		},
		{
			name: "nested path",
			url:  "https://github.com/org/team/project/repo.git",
			want: "repo",
		},
		{
			name: "URL with .git suffix and trailing slash",
			url:  "https://github.com/user/repo.git/",
			want: "repo.git", // Note: trailing slash means .git suffix isn't stripped
		},
		{
			name: "simple name",
			url:  "myrepo",
			want: "myrepo",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := repoNameFromURL(tt.url)
			if got != tt.want {
				t.Errorf("repoNameFromURL(%q) = %q, want %q", tt.url, got, tt.want)
			}
		})
	}
}

func TestConvertToSSHURL(t *testing.T) {
	tests := []struct {
		name string
		url  string
		want string
	}{
		{
			name: "GitHub HTTPS to SSH",
			url:  "https://github.com/user/repo",
			want: "git@github.com:user/repo.git",
		},
		{
			name: "GitHub HTTPS with .git to SSH",
			url:  "https://github.com/user/repo.git",
			want: "git@github.com:user/repo.git",
		},
		{
			name: "GitHub HTTPS with trailing slash",
			url:  "https://github.com/user/repo/",
			want: "git@github.com:user/repo.git",
		},
		{
			name: "GitLab HTTPS to SSH",
			url:  "https://gitlab.com/group/repo",
			want: "git@gitlab.com:group/repo.git",
		},
		{
			name: "GitLab HTTPS with .git to SSH",
			url:  "https://gitlab.com/group/repo.git",
			want: "git@gitlab.com:group/repo.git",
		},
		{
			name: "Generic HTTPS host",
			url:  "https://gitea.example.com/user/repo",
			want: "git@gitea.example.com:user/repo.git",
		},
		{
			name: "SSH URL unchanged",
			url:  "git@github.com:user/repo.git",
			want: "git@github.com:user/repo.git",
		},
		{
			name: "GitHub org/repo",
			url:  "https://github.com/hotschmoe/protectorate",
			want: "git@github.com:hotschmoe/protectorate.git",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := convertToSSHURL(tt.url)
			if got != tt.want {
				t.Errorf("convertToSSHURL(%q) = %q, want %q", tt.url, got, tt.want)
			}
		})
	}
}
