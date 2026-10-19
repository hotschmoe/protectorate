package hub

import (
	"context"
	"errors"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/directory"
)

// directoryService is the DirectoryService handler.
type directoryService struct{ c *coordinator }

func (s directoryService) ListProjects(_ context.Context, req *connect.Request[pb.ListProjectsRequest]) (*connect.Response[pb.ListProjectsResponse], error) {
	res := &pb.ListProjectsResponse{}
	views := s.c.dir.List()
	display := displayNames(views)
	for _, v := range views {
		if want := req.Msg.Sleeve; want != "" && !owns(v, want) {
			continue
		}
		res.Projects = append(res.Projects, projectToProto(v, display))
	}
	return connect.NewResponse(res), nil
}

func (s directoryService) GetProject(_ context.Context, req *connect.Request[pb.GetProjectRequest]) (*connect.Response[pb.GetProjectResponse], error) {
	v, err := s.c.dir.Get(req.Msg.Project)
	if err != nil {
		return nil, directoryErr(err)
	}
	return connect.NewResponse(&pb.GetProjectResponse{Project: projectToProto(v, displayNames(s.c.dir.List()))}), nil
}

func directoryErr(err error) error {
	switch {
	case errors.Is(err, directory.ErrNotFound):
		return connect.NewError(connect.CodeNotFound, err)
	case errors.Is(err, directory.ErrAmbiguous):
		return connect.NewError(connect.CodeInvalidArgument, err)
	}
	return connect.NewError(connect.CodeInternal, err)
}

func owns(v directory.View, sleeve string) bool {
	for _, o := range v.Owners {
		if o.Sleeve == sleeve {
			return true
		}
	}
	return false
}

func displayNames(views []directory.View) map[string]string {
	m := make(map[string]string, len(views))
	for _, v := range views {
		m[v.ID] = v.Display
	}
	return m
}

func projectToProto(v directory.View, display map[string]string) *pb.DirectoryProject {
	p := &pb.DirectoryProject{
		Id: v.ID, Name: v.Name, Display: v.Display, Remote: v.Remote,
		Provides: v.Provides, DependencyCount: uint32(len(v.Deps)),
	}
	for _, o := range v.Owners {
		p.Owners = append(p.Owners, &pb.ProjectOwner{Sleeve: o.Sleeve, Workspace: o.Path, Branch: o.Branch, Head: o.Head})
	}
	edges := func(in []directory.Edge) []*pb.ProjectEdge {
		var out []*pb.ProjectEdge
		for _, e := range in {
			out = append(out, &pb.ProjectEdge{Project: e.Project, Display: display[e.Project], Via: e.Via})
		}
		return out
	}
	p.DependsOn, p.Dependents = edges(v.DependsOn), edges(v.Dependents)
	if r := v.Refs; r != nil {
		p.DefaultBranch, p.LatestTag = r.Default, r.LatestTag()
	}
	return p
}
