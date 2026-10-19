package main

import (
	"fmt"
	"io"
	"strings"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

func whoCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "who [project|sleeve]",
		Short: "Show which sleeves work on which projects and how projects depend on each other",
		Long: `Without an argument, list every project the hub knows: its sleeves and the
projects it depends on. With a sleeve name, list the projects that sleeve works
on; with a project (id, owner/repo or bare name), show its sleeves, provided
packages and edges. The directory is built from each workspace's git remote
and package manifests; nothing is declared by hand.`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			hc, err := o.httpClient()
			if err != nil {
				return err
			}
			cl := protectoratev1connect.NewDirectoryServiceClient(hc, o.hubURL)
			list, err := cl.ListProjects(cmd.Context(), connect.NewRequest(&pb.ListProjectsRequest{}))
			if err != nil {
				return err
			}
			if len(args) == 0 {
				return o.out(cmd).print(list.Msg, func(w io.Writer) { writeProjects(w, list.Msg.Projects) })
			}
			if mine := projectsOf(list.Msg.Projects, args[0]); len(mine) > 0 {
				res := &pb.ListProjectsResponse{Projects: mine}
				return o.out(cmd).print(res, func(w io.Writer) {
					for i, p := range mine {
						if i > 0 {
							fmt.Fprintln(w)
						}
						writeProject(w, p)
					}
				})
			}
			res, err := cl.GetProject(cmd.Context(), connect.NewRequest(&pb.GetProjectRequest{Project: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeProject(w, res.Msg.Project) })
		},
	}
}

func projectsOf(all []*pb.DirectoryProject, sleeve string) []*pb.DirectoryProject {
	var out []*pb.DirectoryProject
	for _, p := range all {
		for _, o := range p.Owners {
			if o.Sleeve == sleeve {
				out = append(out, p)
				break
			}
		}
	}
	return out
}

func ownerNames(p *pb.DirectoryProject) string {
	var n []string
	for _, o := range p.Owners {
		n = append(n, o.Sleeve)
	}
	if len(n) == 0 {
		return "-"
	}
	return strings.Join(n, ",")
}

func edgeNames(es []*pb.ProjectEdge) string {
	var n []string
	for _, e := range es {
		n = append(n, e.Display)
	}
	if len(n) == 0 {
		return "-"
	}
	return strings.Join(n, ",")
}

func writeProjects(w io.Writer, ps []*pb.DirectoryProject) {
	fmt.Fprintln(w, "PROJECT\tSLEEVES\tDEPENDS ON\tDEPENDENTS")
	for _, p := range ps {
		fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", p.Display, ownerNames(p), edgeNames(p.DependsOn), edgeNames(p.Dependents))
	}
}

func writeProject(w io.Writer, p *pb.DirectoryProject) {
	fmt.Fprintf(w, "project\t%s\nid\t%s\n", p.Display, p.Id)
	if p.LatestTag != "" || p.DefaultBranch != "" {
		fmt.Fprintf(w, "release\tbranch %s, latest tag %s\n", orDash(p.DefaultBranch), orDash(p.LatestTag))
	}
	for _, o := range p.Owners {
		branch := o.Branch
		if branch == "" {
			branch = "(detached)"
		}
		head := o.Head
		if len(head) > 8 {
			head = head[:8]
		}
		fmt.Fprintf(w, "sleeve\t%s\t%s %s\t%s\n", o.Sleeve, branch, head, o.Workspace)
	}
	if len(p.Owners) == 0 {
		fmt.Fprintln(w, "sleeve\t(none live)")
	}
	fmt.Fprintf(w, "provides\t%s\n", orDash(strings.Join(p.Provides, ", ")))
	fmt.Fprintf(w, "dependencies\t%d external\n", p.DependencyCount)
	for _, e := range p.DependsOn {
		fmt.Fprintf(w, "depends on\t%s\tvia %s\n", e.Display, strings.Join(e.Via, ", "))
	}
	for _, e := range p.Dependents {
		fmt.Fprintf(w, "dependent\t%s\tvia %s\n", e.Display, strings.Join(e.Via, ", "))
	}
}
