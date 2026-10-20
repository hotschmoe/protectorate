package driver

import (
	"context"
	"fmt"
)

// Empty values leave native defaults in control. Values are native catalog IDs.
type Selection struct {
	Model  string `json:"model,omitempty"`
	Effort string `json:"effort,omitempty"`
}

type Model struct {
	ID            string `json:"model"`
	Name          string `json:"displayName"`
	Default       bool   `json:"isDefault"`
	DefaultEffort string `json:"defaultReasoningEffort"`
	Efforts       []struct {
		Effort string `json:"reasoningEffort"`
	} `json:"supportedReasoningEfforts"`
}

type SelectionState struct {
	Requested Selection
	Effective Selection
	Defaults  Selection
}

func (c *core) Models(context.Context) ([]Model, error)       { return nil, ErrUnsupported }
func (c *core) SetSelection(context.Context, Selection) error { return ErrUnsupported }
func (c *core) Selection() SelectionState                     { return SelectionState{} }

func CheckSelection(h Harness, sel Selection) error {
	if sel != (Selection{}) && h != Codex {
		return fmt.Errorf("%w: model/effort selection for %s", ErrUnsupported, h)
	}
	return nil
}

// Catalog queries native discovery without creating a thread or starting a turn.
func Catalog(ctx context.Context, h Harness, cwd string, opts Options) ([]Model, error) {
	if h != Codex {
		return nil, fmt.Errorf("%w: model catalog for %s", ErrUnsupported, h)
	}
	s, err := startCodex(ctx, cwd, opts)
	if err != nil {
		return nil, err
	}
	defer s.Close()
	return s.Models(ctx)
}

func resolveSelection(models []Model, defaults, requested Selection) (Selection, error) {
	out := requested
	if out.Model == "" {
		out.Model = defaults.Model
	}
	for _, model := range models {
		if model.ID != out.Model {
			continue
		}
		if out.Effort == "" {
			out.Effort = model.DefaultEffort
			if requested.Model == "" && defaults.Effort != "" {
				out.Effort = defaults.Effort
			}
		}
		if out.Effort == "" {
			return out, nil
		}
		for _, effort := range model.Efforts {
			if effort.Effort == out.Effort {
				return out, nil
			}
		}
		return Selection{}, fmt.Errorf("model %q does not advertise effort %q", out.Model, out.Effort)
	}
	return Selection{}, fmt.Errorf("model %q is not in the native catalog", out.Model)
}

func ValidateSelection(models []Model, requested Selection) error {
	defaults := Selection{}
	for _, model := range models {
		if model.Default {
			defaults.Model = model.ID
			defaults.Effort = model.DefaultEffort
			break
		}
	}
	_, err := resolveSelection(models, defaults, requested)
	return err
}
