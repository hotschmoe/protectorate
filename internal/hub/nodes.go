package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"net"
	"net/http"
	"slices"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/node"
)

const pollConcurrency = 16

type NodeOptions struct {
	Interval         time.Duration // poll period, default 30s
	StaleAfter       time.Duration // good data older than this is UNREACHABLE, default 2*Interval
	FetchTimeout     time.Duration // per-node bound, default 15s
	Client           *http.Client  // used for remote nodes, default a plain client
	Token            string        // bearer token sent to node agents, if any
	DiskAlertPercent float64       // default 90
	MemAlertPercent  float64       // default 90
	Now              func() time.Time
	// Peers lists the machines enrolled in the tailnet; it lets the monitor
	// notice an invited node that joined under another name. Nil when there
	// is no tailnet.
	Peers func(ctx context.Context) ([]Peer, error)
}

// Peer is a machine enrolled in the tailnet and the id of the pre-auth key it
// enrolled with (0 if none).
type Peer struct {
	Name  string
	KeyID uint64
}

func (o NodeOptions) withDefaults() NodeOptions {
	if o.Interval <= 0 {
		o.Interval = 30 * time.Second
	}
	if o.StaleAfter <= 0 {
		o.StaleAfter = 2 * o.Interval
	}
	if o.FetchTimeout <= 0 {
		o.FetchTimeout = 15 * time.Second
	}
	if o.Client == nil {
		o.Client = &http.Client{}
	}
	if o.DiskAlertPercent <= 0 {
		o.DiskAlertPercent = 90
	}
	if o.MemAlertPercent <= 0 {
		o.MemAlertPercent = 90
	}
	if o.Now == nil {
		o.Now = time.Now
	}
	return o
}

type nodeState struct {
	good        *pb.NodeMetrics
	goodAt      time.Time
	attempted   bool
	lastErr     string
	downEmitted bool
}

// NodeMonitor polls every node's metrics, caches the last good result and
// derives freshness and alerts. The hub's own node is collected in-process.
type NodeMonitor struct {
	opts  NodeOptions
	inv   *Inventory
	local *node.Collector
	self  *pb.NodeInfo

	publish func(kind, subject string, payload *structpb.Struct)
	// localSleeves lists the hub's sleeves for its own node's metrics.
	localSleeves func() []*pb.Sleeve
	// fetchRemote is replaced in tests.
	fetchRemote func(ctx context.Context, n *pb.NodeInfo) (*pb.NodeMetrics, error)

	mu     sync.Mutex
	state  map[string]*nodeState
	runCtx context.Context
	// mismatch maps an invited node's name to the tailnet name it joined as.
	mismatch map[string]string
}

// NewNodeMonitor builds the monitor. self describes the hub's own node and
// must carry its name; local collects its metrics.
func NewNodeMonitor(inv *Inventory, self *pb.NodeInfo, local *node.Collector, opts NodeOptions) *NodeMonitor {
	m := &NodeMonitor{opts: opts.withDefaults(), inv: inv, local: local, self: self, state: map[string]*nodeState{}, mismatch: map[string]string{}}
	m.fetchRemote = m.fetchHTTP
	return m
}

// LocalNodeInfo describes this machine as the hub's own node.
func LocalNodeInfo(name, osName, arch string) *pb.NodeInfo {
	return &pb.NodeInfo{Name: strings.ToLower(name), Address: "local", Os: osName, Arch: arch, Tier: TierServer, Local: true}
}

// Run polls immediately and then every Interval until ctx is done.
func (m *NodeMonitor) Run(ctx context.Context) {
	m.mu.Lock()
	m.runCtx = ctx
	m.mu.Unlock()
	if m.local != nil {
		go m.local.Run(ctx)
	}
	m.pollAll(ctx)
	tick := time.NewTicker(m.opts.Interval)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			m.pollAll(ctx)
		}
	}
}

func (m *NodeMonitor) targets() []*pb.NodeInfo {
	return append([]*pb.NodeInfo{m.self}, m.inv.List()...)
}

func (m *NodeMonitor) pollAll(ctx context.Context) {
	targets := m.targets()
	names := make(map[string]bool, len(targets))
	var wg sync.WaitGroup
	sem := make(chan struct{}, pollConcurrency)
	for _, n := range targets {
		names[n.Name] = true
		wg.Add(1)
		sem <- struct{}{}
		go func() {
			defer wg.Done()
			defer func() { <-sem }()
			m.pollOne(ctx, n)
		}()
	}
	wg.Wait()
	m.mu.Lock()
	for name := range m.state {
		if !names[name] {
			delete(m.state, name)
		}
	}
	m.mu.Unlock()
	m.checkInvited(ctx, targets)
}

// checkInvited finds invited nodes that enrolled under a different tailnet
// name than they were invited as.
func (m *NodeMonitor) checkInvited(ctx context.Context, targets []*pb.NodeInfo) {
	if m.opts.Peers == nil {
		return
	}
	var invited []*pb.NodeInfo
	for _, n := range targets {
		if n.Invited && n.InviteKeyId != 0 {
			invited = append(invited, n)
		}
	}
	m.mu.Lock()
	for name := range m.mismatch {
		if !slices.ContainsFunc(invited, func(n *pb.NodeInfo) bool { return n.Name == name }) {
			delete(m.mismatch, name)
		}
	}
	m.mu.Unlock()
	if len(invited) == 0 {
		return
	}
	peers, err := m.opts.Peers(ctx)
	if err != nil {
		log.Printf("node invites: list tailnet peers: %v", err)
		return
	}
	for _, n := range invited {
		i := slices.IndexFunc(peers, func(p Peer) bool { return p.KeyID == n.InviteKeyId })
		if i < 0 || peers[i].Name == n.Name {
			continue
		}
		as := peers[i].Name
		m.mu.Lock()
		prev := m.mismatch[n.Name]
		m.mismatch[n.Name] = as
		m.mu.Unlock()
		if prev != as && m.publish != nil {
			p, _ := structpb.NewStruct(map[string]any{"invited_as": n.Name, "joined_as": as})
			m.publish("node.name_mismatch", n.Name, p)
		}
	}
}

func (m *NodeMonitor) pollOne(ctx context.Context, n *pb.NodeInfo) {
	fctx, cancel := context.WithTimeout(ctx, m.opts.FetchTimeout)
	defer cancel()
	var metrics *pb.NodeMetrics
	var err error
	if n.Local {
		metrics, err = m.local.Collect(fctx)
		if err == nil && m.localSleeves != nil {
			metrics = proto.Clone(metrics).(*pb.NodeMetrics)
			metrics.Sleeves = m.localSleeves()
		}
	} else {
		metrics, err = m.fetchRemote(fctx, n)
	}
	if ctx.Err() != nil {
		return
	}

	joined := err == nil && n.Invited
	if joined {
		n = proto.Clone(n).(*pb.NodeInfo)
		n.Invited, n.InviteKeyId = false, 0
	}
	now := m.opts.Now()
	m.mu.Lock()
	st := m.state[n.Name]
	if st == nil {
		st = &nodeState{}
		m.state[n.Name] = st
	}
	st.attempted = true
	if err != nil {
		st.lastErr = err.Error()
	} else {
		st.good, st.goodAt, st.lastErr, st.downEmitted = metrics, now, "", false
	}
	status := m.statusLocked(n, st, now)
	emitDown := status.Freshness == pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE && !st.downEmitted
	if emitDown {
		st.downEmitted = true
	}
	m.mu.Unlock()

	if joined {
		if cerr := m.inv.ClearInvited(n.Name); cerr != nil {
			log.Printf("node %s: clear invited: %v", n.Name, cerr)
		} else if m.publish != nil {
			p, _ := structpb.NewStruct(map[string]any{"address": n.Address})
			m.publish("node.joined", n.Name, p)
		}
		m.mu.Lock()
		delete(m.mismatch, n.Name)
		m.mu.Unlock()
	}
	if err == nil && !n.Local {
		if perr := m.inv.SetPlatform(n.Name, metrics.Os, metrics.Arch); perr != nil {
			log.Printf("node %s: record platform: %v", n.Name, perr)
		}
	}
	if m.publish == nil {
		return
	}
	switch {
	case err == nil:
		m.publish("node.metrics", n.Name, metricsSummary(status))
	case emitDown:
		m.publish("node.unreachable", n.Name, unreachableSummary(status))
	}
}

// agentClient talks to one node agent, presenting the node token if any.
func (m *NodeMonitor) agentClient(n *pb.NodeInfo, opts ...connect.ClientOption) protectoratev1connect.NodeAgentServiceClient {
	if m.opts.Token != "" {
		opts = append(opts, connect.WithInterceptors(bearerInterceptor{token: m.opts.Token}))
	}
	return protectoratev1connect.NewNodeAgentServiceClient(m.opts.Client, AgentURL(n.Address), opts...)
}

type bearerInterceptor struct{ token string }

func (b bearerInterceptor) WrapUnary(next connect.UnaryFunc) connect.UnaryFunc {
	return func(ctx context.Context, req connect.AnyRequest) (connect.AnyResponse, error) {
		req.Header().Set("Authorization", "Bearer "+b.token)
		return next(ctx, req)
	}
}

func (b bearerInterceptor) WrapStreamingClient(next connect.StreamingClientFunc) connect.StreamingClientFunc {
	return func(ctx context.Context, spec connect.Spec) connect.StreamingClientConn {
		conn := next(ctx, spec)
		conn.RequestHeader().Set("Authorization", "Bearer "+b.token)
		return conn
	}
}

func (b bearerInterceptor) WrapStreamingHandler(next connect.StreamingHandlerFunc) connect.StreamingHandlerFunc {
	return next
}

// fetchHTTP gives the first attempt half the fetch budget. A node that died
// leaves a pooled connection that never errors, so on a transport failure the
// pool is dropped and the rest of the budget is spent on a fresh connection.
func (m *NodeMonitor) fetchHTTP(ctx context.Context, n *pb.NodeInfo) (*pb.NodeMetrics, error) {
	call := func(ctx context.Context) (*pb.NodeMetrics, error) {
		res, err := m.agentClient(n, connect.WithProtoJSON()).GetMetrics(ctx, connect.NewRequest(&pb.GetMetricsRequest{}))
		if err != nil {
			return nil, err
		}
		return res.Msg, nil
	}
	actx, cancel := context.WithTimeout(ctx, m.opts.FetchTimeout/2)
	metrics, err := call(actx)
	cancel()
	if err == nil || ctx.Err() != nil || !transportFailure(err) {
		return metrics, err
	}
	m.opts.Client.CloseIdleConnections()
	return call(ctx)
}

func transportFailure(err error) bool {
	switch connect.CodeOf(err) {
	case connect.CodeUnavailable, connect.CodeDeadlineExceeded:
		return true
	}
	return false
}

// AgentURL turns a tailnet name or address into the node agent's base URL.
func AgentURL(addr string) string {
	if strings.Contains(addr, "://") {
		return strings.TrimRight(addr, "/")
	}
	if _, _, err := net.SplitHostPort(addr); err != nil {
		addr = net.JoinHostPort(strings.Trim(addr, "[]"), fmt.Sprint(node.DefaultPort))
	}
	return "http://" + addr
}

// Statuses returns the hub's own node first, then the inventory by name.
func (m *NodeMonitor) Statuses() []*pb.NodeStatus {
	now := m.opts.Now()
	targets := m.targets()
	out := make([]*pb.NodeStatus, len(targets))
	m.mu.Lock()
	defer m.mu.Unlock()
	for i, n := range targets {
		out[i] = m.statusLocked(n, m.state[n.Name], now)
	}
	return out
}

func (m *NodeMonitor) Status(name string) (*pb.NodeStatus, error) {
	name = strings.ToLower(name)
	var info *pb.NodeInfo
	if name == m.self.Name {
		info = m.self
	} else {
		var err error
		if info, err = m.inv.Get(name); err != nil {
			return nil, err
		}
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.statusLocked(info, m.state[name], m.opts.Now()), nil
}

func (m *NodeMonitor) statusLocked(n *pb.NodeInfo, st *nodeState, now time.Time) *pb.NodeStatus {
	s := &pb.NodeStatus{Node: n}
	if n.Invited {
		s.Freshness = pb.NodeFreshness_NODE_FRESHNESS_INVITED
		s.Error = "invited, not yet joined"
		if as := m.mismatch[n.Name]; as != "" {
			s.Error = fmt.Sprintf("joined the tailnet as %q, not the invited name %q", as, n.Name)
			s.Alerts = []*pb.NodeAlert{{
				Kind: pb.AlertKind_ALERT_KIND_NAME_MISMATCH, Subject: as,
				Message: fmt.Sprintf("invited as %q but joined as %q: `node rm %s`, then `node add %s --addr %s`", n.Name, as, n.Name, as, as),
			}}
		}
		return s
	}
	if st == nil || !st.attempted {
		s.Freshness = pb.NodeFreshness_NODE_FRESHNESS_UNSPECIFIED
		s.Error = "awaiting first poll"
		return s
	}
	s.Error = st.lastErr
	if st.good == nil {
		s.Freshness = pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE
	} else {
		age := now.Sub(st.goodAt)
		s.Metrics = st.good
		s.FetchedAt = timestamppb.New(st.goodAt)
		s.Age = durationpb.New(age)
		switch {
		case age > m.opts.StaleAfter:
			s.Freshness = pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE
		case st.lastErr != "":
			s.Freshness = pb.NodeFreshness_NODE_FRESHNESS_STALE
		default:
			s.Freshness = pb.NodeFreshness_NODE_FRESHNESS_FRESH
		}
	}
	s.Alerts = alertsFor(s, m.opts.DiskAlertPercent, m.opts.MemAlertPercent)
	return s
}

func alertsFor(s *pb.NodeStatus, diskPct, memPct float64) []*pb.NodeAlert {
	var out []*pb.NodeAlert
	if s.Freshness == pb.NodeFreshness_NODE_FRESHNESS_UNREACHABLE {
		out = append(out, &pb.NodeAlert{Kind: pb.AlertKind_ALERT_KIND_UNREACHABLE, Message: "node is unreachable: " + s.Error})
	}
	m := s.Metrics
	if m == nil {
		return out
	}
	for _, f := range m.Filesystems {
		if f.UsedPercent >= diskPct {
			out = append(out, &pb.NodeAlert{
				Kind: pb.AlertKind_ALERT_KIND_DISK_PERCENT, Subject: f.Mount, Value: f.UsedPercent, Threshold: diskPct,
				Message: fmt.Sprintf("%s is %.0f%% full", f.Mount, f.UsedPercent),
			})
		}
	}
	if mem := m.Memory; mem != nil && mem.UsedPercent >= memPct {
		out = append(out, &pb.NodeAlert{
			Kind: pb.AlertKind_ALERT_KIND_MEM_PRESSURE, Value: mem.UsedPercent, Threshold: memPct,
			Message: fmt.Sprintf("memory is %.0f%% used", mem.UsedPercent),
		})
	}
	return out
}

func alertKinds(alerts []*pb.NodeAlert) []any {
	out := make([]any, 0, len(alerts))
	for _, a := range alerts {
		out = append(out, strings.ToLower(strings.TrimPrefix(a.Kind.String(), "ALERT_KIND_")))
	}
	return out
}

func metricsSummary(s *pb.NodeStatus) *structpb.Struct {
	m := s.Metrics
	v := map[string]any{
		"freshness":  strings.ToLower(strings.TrimPrefix(s.Freshness.String(), "NODE_FRESHNESS_")),
		"alerts":     alertKinds(s.Alerts),
		"gpu_count":  float64(len(m.GetGpus())),
		"containers": float64(len(m.GetEngine().GetContainers())),
	}
	if c := m.GetCpu(); c != nil {
		v["cpu_percent"] = c.UsagePercent
	}
	if mem := m.GetMemory(); mem != nil {
		v["mem_percent"] = mem.UsedPercent
	}
	if w := node.WorstFilesystem(m); w != nil {
		v["worst_disk_percent"] = w.UsedPercent
		v["worst_disk_mount"] = w.Mount
	}
	p, _ := structpb.NewStruct(v)
	return p
}

func unreachableSummary(s *pb.NodeStatus) *structpb.Struct {
	p, _ := structpb.NewStruct(map[string]any{"error": s.Error, "address": s.Node.Address})
	return p
}

// kick polls one node now, so a freshly added node does not wait for the
// next interval.
func (m *NodeMonitor) kick(n *pb.NodeInfo) {
	m.mu.Lock()
	ctx := m.runCtx
	m.mu.Unlock()
	if ctx != nil {
		go m.pollOne(ctx, n)
	}
}

func (h *Hub) ListNodes(context.Context, *connect.Request[pb.ListNodesRequest]) (*connect.Response[pb.ListNodesResponse], error) {
	return connect.NewResponse(&pb.ListNodesResponse{Nodes: h.nodes.Statuses()}), nil
}

func (h *Hub) GetNode(_ context.Context, req *connect.Request[pb.GetNodeRequest]) (*connect.Response[pb.NodeStatus], error) {
	s, err := h.nodes.Status(req.Msg.Name)
	if err != nil {
		return nil, nodeErr(err)
	}
	return connect.NewResponse(s), nil
}

func (h *Hub) AddNode(_ context.Context, req *connect.Request[pb.AddNodeRequest]) (*connect.Response[pb.NodeInfo], error) {
	add := h.nodes.inv.Add
	if req.Msg.Update {
		add = h.nodes.inv.Upsert
	}
	info, err := add(req.Msg.Node)
	if err != nil {
		return nil, nodeErr(err)
	}
	h.nodes.kick(info)
	return connect.NewResponse(info), nil
}

func (h *Hub) RemoveNode(_ context.Context, req *connect.Request[pb.RemoveNodeRequest]) (*connect.Response[pb.RemoveNodeResponse], error) {
	if strings.EqualFold(req.Msg.Name, h.nodes.self.Name) {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("the hub's own node cannot be removed"))
	}
	if err := h.nodes.inv.Remove(req.Msg.Name); err != nil {
		return nil, nodeErr(err)
	}
	return connect.NewResponse(&pb.RemoveNodeResponse{}), nil
}

func nodeErr(err error) error {
	switch {
	case errors.Is(err, ErrNodeNotFound):
		return connect.NewError(connect.CodeNotFound, err)
	case errors.Is(err, ErrNodeExists):
		return connect.NewError(connect.CodeAlreadyExists, err)
	case errors.Is(err, ErrInvalidNode):
		return connect.NewError(connect.CodeInvalidArgument, err)
	}
	return connect.NewError(connect.CodeInternal, err)
}

// info returns the inventory record of a node, or the hub's own.
func (m *NodeMonitor) info(name string) (*pb.NodeInfo, error) {
	name = strings.ToLower(name)
	if name == m.self.Name {
		return m.self, nil
	}
	return m.inv.Get(name)
}
