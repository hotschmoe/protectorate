package hub

import (
	"context"
	"fmt"
	"os"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/driver"
)

func selectionFromProto(s *pb.ModelSelection) driver.Selection {
	return driver.Selection{Model: s.GetModel(), Effort: s.GetEffort()}
}
func selectionToProto(s driver.Selection) *pb.ModelSelection {
	return &pb.ModelSelection{Model: s.Model, Effort: s.Effort}
}
func settingsToProto(s driver.SelectionState) *pb.ModelSettings {
	return &pb.ModelSettings{Requested: selectionToProto(s.Requested), Effective: selectionToProto(s.Effective), NativeDefaults: selectionToProto(s.Defaults)}
}

// Validate before a create/resleeve can stop an existing session or clone a repo.
func (h *Hub) validateSelection(ctx context.Context, harness driver.Harness, selection driver.Selection) error {
	if err := driver.CheckSelection(harness, selection); err != nil {
		return connect.NewError(connect.CodeInvalidArgument, err)
	}
	if selection == (driver.Selection{}) {
		return nil
	}
	ctx, cancel := context.WithTimeout(ctx, driverOpenTimeout)
	defer cancel()
	opts, err := h.sleeves.CatalogOptions()
	if err != nil {
		return sleeveErr(err)
	}
	models, err := driver.Catalog(ctx, harness, os.TempDir(), opts)
	if err != nil {
		return connect.NewError(connect.CodeFailedPrecondition, err)
	}
	if err := driver.ValidateSelection(models, selection); err != nil {
		return connect.NewError(connect.CodeInvalidArgument, err)
	}
	return nil
}

func (c chatService) ListModels(ctx context.Context, req *connect.Request[pb.ListModelsRequest]) (*connect.Response[pb.ListModelsResponse], error) {
	if c.h.sleeves == nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("sleeves unavailable"))
	}
	ctx, cancel := context.WithTimeout(ctx, driverOpenTimeout)
	defer cancel()
	h := driver.Harness(req.Msg.Harness)
	var models []driver.Model
	var err error
	if req.Msg.Sleeve != "" {
		if err := ownSleeveOnly(ctx, req.Msg.Sleeve); err != nil {
			return nil, err
		}
		info, err := c.h.sleeves.Get(req.Msg.Sleeve)
		if err != nil {
			return nil, sleeveErr(err)
		}
		h = driver.Harness(info.Harness)
	}
	if h != driver.Codex {
		return connect.NewResponse(&pb.ListModelsResponse{Reason: fmt.Sprintf("model/effort controls are not implemented for %s", h)}), nil
	}
	if req.Msg.Sleeve != "" {
		s, e := c.sleeve(req.Msg.Sleeve)
		if e != nil {
			return nil, e
		}
		s.opMu.Lock()
		defer s.opMu.Unlock()
		if err := s.ensureOpen(ctx); err != nil {
			return nil, err
		}
		models, err = s.liveSession().Models(ctx)
	} else {
		opts, e := c.h.sleeves.CatalogOptions()
		if e != nil {
			return nil, sleeveErr(e)
		}
		models, err = driver.Catalog(ctx, h, os.TempDir(), opts)
	}
	if err != nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, err)
	}
	out := &pb.ListModelsResponse{Supported: true, LiveUpdate: true}
	for _, model := range models {
		m := &pb.ModelOption{Model: model.ID, Name: model.Name, DefaultEffort: model.DefaultEffort, IsDefault: model.Default}
		for _, effort := range model.Efforts {
			m.Efforts = append(m.Efforts, effort.Effort)
		}
		out.Models = append(out.Models, m)
	}
	return connect.NewResponse(out), nil
}

func (c chatService) SetSelection(ctx context.Context, req *connect.Request[pb.SetSelectionRequest]) (*connect.Response[pb.SetSelectionResponse], error) {
	s, err := c.sleeve(req.Msg.Sleeve)
	if err != nil {
		return nil, err
	}
	return s.setSelection(ctx, req.Msg.Selection)
}

func (s *chatSleeve) setSelection(ctx context.Context, selection *pb.ModelSelection) (*connect.Response[pb.SetSelectionResponse], error) {
	ctx, cancel := context.WithTimeout(ctx, driverOpenTimeout)
	defer cancel()
	s.opMu.Lock()
	defer s.opMu.Unlock()
	s.mu.Lock()
	busy := s.hubOwned || s.turn || s.compacting || len(s.approvals) > 0
	s.mu.Unlock()
	if busy {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("selection changes require an idle chat"))
	}
	if err := s.ensureOpen(ctx); err != nil {
		return nil, err
	}
	s.pmu.Lock()
	defer s.pmu.Unlock()
	sess := s.liveSession()
	if !sess.Caps().Selection {
		return nil, connect.NewError(connect.CodeUnimplemented, driver.ErrUnsupported)
	}
	updateErr := sess.SetSelection(ctx, selectionFromProto(selection))
	st := sess.Selection()
	s.mu.Lock()
	defer s.mu.Unlock()
	if updateErr != nil && st.Effective == (driver.Selection{Model: s.model, Effort: s.effort}) {
		return nil, connect.NewError(connect.CodeFailedPrecondition, updateErr)
	}
	s.requested, s.defaults, s.model, s.effort = st.Requested, st.Defaults, st.Effective.Model, st.Effective.Effort
	s.appendLocked(func(e *pb.ChatEvent) { e.Body = &pb.ChatEvent_Settings{Settings: settingsToProto(st)} })
	if err := s.persistLocked(true); err != nil {
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("native selection applied, persistence failed: %w", err))
	}
	s.publishLocked(true)
	if updateErr != nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, fmt.Errorf("native effective settings changed but request was not confirmed: %w", updateErr))
	}
	return connect.NewResponse(&pb.SetSelectionResponse{Chat: s.statusLocked()}), nil
}
