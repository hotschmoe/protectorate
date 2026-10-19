package main

import (
	"strings"
	"testing"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func TestWriteProjects(t *testing.T) {
	lib := &pb.DirectoryProject{Id: "github.com/a/lib", Display: "a/lib", Provides: []string{"go:github.com/a/lib"},
		Owners:     []*pb.ProjectOwner{{Sleeve: "libdev", Workspace: "/w/lib", Branch: "main", Head: "0123456789abcdef"}},
		Dependents: []*pb.ProjectEdge{{Project: "github.com/a/api", Display: "a/api", Via: []string{"go:github.com/a/lib"}}},
		LatestTag:  "v1.0.0", DefaultBranch: "main"}
	api := &pb.DirectoryProject{Id: "github.com/a/api", Display: "a/api", DependencyCount: 3,
		DependsOn: []*pb.ProjectEdge{{Project: lib.Id, Display: "a/lib", Via: []string{"go:github.com/a/lib"}}}}

	tests := []struct {
		name  string
		write func(sb *strings.Builder)
		want  []string
	}{
		{"list", func(sb *strings.Builder) { writeProjects(sb, []*pb.DirectoryProject{lib, api}) },
			[]string{"PROJECT\tSLEEVES\tDEPENDS ON\tDEPENDENTS", "a/lib\tlibdev\t-\ta/api", "a/api\t-\ta/lib\t-"}},
		{"detail", func(sb *strings.Builder) { writeProject(sb, lib) },
			[]string{"project\ta/lib", "release\tbranch main, latest tag v1.0.0", "sleeve\tlibdev\tmain 01234567\t/w/lib", "dependent\ta/api\tvia go:github.com/a/lib"}},
		{"detail without sleeves", func(sb *strings.Builder) { writeProject(sb, api) },
			[]string{"sleeve\t(none live)", "dependencies\t3 external", "depends on\ta/lib"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var sb strings.Builder
			tt.write(&sb)
			for _, want := range tt.want {
				if !strings.Contains(sb.String(), want) {
					t.Errorf("output lacks %q:\n%s", want, sb.String())
				}
			}
		})
	}
	if got := projectsOf([]*pb.DirectoryProject{lib, api}, "libdev"); len(got) != 1 || got[0] != lib {
		t.Fatalf("projectsOf = %v", got)
	}
}
