package main

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"reflect"
	"sort"
	"strings"
)

const (
	codeParse          = -32700
	codeInvalidRequest = -32600
	codeMethodNotFound = -32601
	codeInvalidParams  = -32602
	codeInternal       = -32603
	codeAlreadyExists  = -32001
)

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func (e *rpcError) Error() string { return e.Message }

type method struct {
	name, desc string
	in         reflect.Type
	call       func(ctx context.Context, params json.RawMessage) (any, error)
}

type registry struct{ methods map[string]*method }

func register[P, R any](r *registry, name, desc string, fn func(context.Context, P) (R, error)) {
	if r.methods == nil {
		r.methods = map[string]*method{}
	}
	r.methods[name] = &method{
		name: name, desc: desc, in: reflect.TypeOf((*P)(nil)).Elem(),
		call: func(ctx context.Context, raw json.RawMessage) (any, error) {
			var p P
			if len(raw) > 0 {
				dec := json.NewDecoder(strings.NewReader(string(raw)))
				dec.DisallowUnknownFields()
				if err := dec.Decode(&p); err != nil {
					return nil, &rpcError{codeInvalidParams, err.Error()}
				}
			}
			return fn(ctx, p)
		},
	}
}

type request struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type response struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

func (r *registry) handler(w http.ResponseWriter, req *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	var rq request
	if err := json.NewDecoder(req.Body).Decode(&rq); err != nil {
		json.NewEncoder(w).Encode(response{JSONRPC: "2.0", ID: json.RawMessage("null"), Error: &rpcError{codeParse, err.Error()}})
		return
	}
	resp := response{JSONRPC: "2.0", ID: rq.ID}
	if resp.ID == nil {
		resp.ID = json.RawMessage("null")
	}
	m, ok := r.methods[rq.Method]
	if !ok {
		resp.Error = &rpcError{codeMethodNotFound, "method not found: " + rq.Method}
	} else if result, err := m.call(req.Context(), rq.Params); err != nil {
		var re *rpcError
		if !errors.As(err, &re) {
			re = &rpcError{codeInternal, err.Error()}
		}
		resp.Error = re
	} else {
		resp.Result = result
	}
	json.NewEncoder(w).Encode(resp)
}

func (r *registry) tools() []map[string]any {
	var out []map[string]any
	for _, m := range r.methods {
		out = append(out, map[string]any{
			"name":        strings.ReplaceAll(m.name, ".", "_"),
			"description": m.desc,
			"inputSchema": schemaOf(m.in),
		})
	}
	sort.Slice(out, func(i, j int) bool { return out[i]["name"].(string) < out[j]["name"].(string) })
	return out
}

func schemaOf(t reflect.Type) map[string]any {
	props := map[string]any{}
	var required []string
	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		name, opts, _ := strings.Cut(f.Tag.Get("json"), ",")
		if name == "" || name == "-" {
			continue
		}
		p := map[string]any{"type": jsonKind(f.Type)}
		if d := f.Tag.Get("desc"); d != "" {
			p["description"] = d
		}
		props[name] = p
		if !strings.Contains(opts, "omitempty") {
			required = append(required, name)
		}
	}
	s := map[string]any{"type": "object", "properties": props, "additionalProperties": false}
	if len(required) > 0 {
		s["required"] = required
	}
	return s
}

func jsonKind(t reflect.Type) string {
	switch t.Kind() {
	case reflect.String:
		return "string"
	case reflect.Bool:
		return "boolean"
	case reflect.Float32, reflect.Float64:
		return "number"
	case reflect.Slice:
		return "array"
	case reflect.Struct, reflect.Map:
		return "object"
	default:
		return "integer"
	}
}
