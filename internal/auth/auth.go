package auth

import (
	"context"
	"crypto/subtle"
	"errors"
	"fmt"
	"time"

	"google.golang.org/protobuf/reflect/protoreflect"
)

// Errors returned by Authorizer. ErrUnauthenticated means the credential is
// missing or not valid; *DeniedError means it is valid but out of scope.
var ErrUnauthenticated = errors.New("missing or invalid bearer token")

type DeniedError struct {
	Name string
	Need string
}

func (e *DeniedError) Error() string {
	return fmt.Sprintf("token %q lacks the required scope: %s", e.Name, e.Need)
}

// Principal is an authenticated caller.
type Principal struct {
	ID     string
	Name   string
	Owner  bool
	Scopes []string
}

// Actor is the audit label: "owner" or "token:<name>".
func (p *Principal) Actor() string {
	if p.Owner {
		return "owner"
	}
	return "token:" + p.Name
}

type ctxKey int

const (
	principalKey ctxKey = iota
	tailnetKey
)

// WithTailnet records the WhoIs identity that admitted a request.
func WithTailnet(ctx context.Context, identity string) context.Context {
	return context.WithValue(ctx, tailnetKey, identity)
}

func TailnetFrom(ctx context.Context) string {
	s, _ := ctx.Value(tailnetKey).(string)
	return s
}

func WithPrincipal(ctx context.Context, p *Principal) context.Context {
	return context.WithValue(ctx, principalKey, p)
}

func PrincipalFrom(ctx context.Context) *Principal {
	p, _ := ctx.Value(principalKey).(*Principal)
	return p
}

// Authorizer is the single decision point every entry point uses: the Connect
// interceptor, the WebSocket terminal and, behind the tailnet WhoIs gate, the
// tailnet listener. The owner token has full access and never expires.
type Authorizer struct {
	owner string
	store *Store
	Audit *Audit
	Now   func() time.Time
}

func New(owner string, store *Store, audit *Audit) *Authorizer {
	if store == nil {
		store = NewMemoryStore()
	}
	return &Authorizer{owner: owner, store: store, Audit: audit, Now: time.Now}
}

func (a *Authorizer) Store() *Store { return a.store }

// Authenticate resolves a bearer secret. Unknown, expired and revoked tokens
// all yield ErrUnauthenticated.
func (a *Authorizer) Authenticate(secret string) (*Principal, error) {
	if secret == "" {
		return nil, ErrUnauthenticated
	}
	ownerOK := subtle.ConstantTimeCompare([]byte(secret), []byte(a.owner)) == 1
	t, err := a.store.Lookup(secret, a.Now())
	if ownerOK && a.owner != "" {
		return &Principal{Name: "owner", Owner: true}, nil
	}
	if err != nil {
		return nil, ErrUnauthenticated
	}
	return &Principal{ID: t.ID, Name: t.Name, Scopes: t.Scopes}, nil
}

// Method looks up a Connect procedure; unknown procedures are denied.
func (a *Authorizer) Method(procedure string) (Method, bool) {
	ms, err := Methods()
	if err != nil {
		return Method{}, false
	}
	m, ok := ms[procedure]
	return m, ok
}

// AuthorizeProcedure checks p against procedure.
func (a *Authorizer) AuthorizeProcedure(p *Principal, procedure string) error {
	m, ok := a.Method(procedure)
	if !ok {
		if p.Owner {
			return nil
		}
		return &DeniedError{Name: p.Name, Need: "owner (unknown procedure " + procedure + ")"}
	}
	if p.Owner {
		return nil
	}
	if m.OwnerOnly {
		return &DeniedError{Name: p.Name, Need: "owner token (" + m.Tool + " is owner-only)"}
	}
	if !scopesAllow(p.Scopes, m) {
		return &DeniedError{Name: p.Name, Need: needed(m)}
	}
	return nil
}

// AuthorizeTerminal checks p for a WebSocket terminal attach.
func (a *Authorizer) AuthorizeTerminal(p *Principal, readOnly bool) error {
	if p.Owner || scopesAllowTerminal(p.Scopes, readOnly) {
		return nil
	}
	need := ScopeTerminal
	if readOnly {
		need = ScopeTerminal + " or " + ScopeTerminalReadonly
	}
	return &DeniedError{Name: p.Name, Need: need}
}

// Record audits one call outcome.
func (a *Authorizer) Record(ctx context.Context, p *Principal, procedure, subject string, err error, code string) {
	e := Entry{
		Actor:     "unauthenticated",
		Tailnet:   TailnetFrom(ctx),
		Procedure: procedure,
		Subject:   subject,
		Outcome:   OutcomeOK,
		Code:      code,
	}
	if p != nil {
		e.Actor, e.TokenID = p.Actor(), p.ID
	}
	if err != nil {
		e.Outcome = OutcomeError
		var d *DeniedError
		if errors.Is(err, ErrUnauthenticated) || errors.As(err, &d) {
			e.Outcome = OutcomeDenied
		}
	}
	a.Audit.Write(e)
}

// Subject extracts the audit subject of a request message: its "name" or "id"
// string field, or failing that the same field of one nested message such as
// AddNodeRequest.node. Nothing else from the body is read.
func Subject(msg any) string {
	m, ok := msg.(protoreflect.ProtoMessage)
	if !ok || m == nil {
		return ""
	}
	return subjectOf(m.ProtoReflect(), true)
}

func subjectOf(r protoreflect.Message, nested bool) string {
	for _, n := range []protoreflect.Name{"name", "id"} {
		fd := r.Descriptor().Fields().ByName(n)
		if fd != nil && fd.Kind() == protoreflect.StringKind && !fd.IsList() && !fd.IsMap() {
			return r.Get(fd).String()
		}
	}
	if !nested {
		return ""
	}
	fields := r.Descriptor().Fields()
	for i := 0; i < fields.Len(); i++ {
		fd := fields.Get(i)
		if fd.Kind() == protoreflect.MessageKind && !fd.IsList() && !fd.IsMap() && r.Has(fd) {
			if s := subjectOf(r.Get(fd).Message(), false); s != "" {
				return s
			}
		}
	}
	return ""
}
