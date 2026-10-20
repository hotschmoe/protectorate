package hub

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/netip"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
)

type deviceIPKey struct{}
type deviceWindow struct {
	start time.Time
	count int
}
type deviceLimiter struct {
	mu      sync.Mutex
	windows map[string]deviceWindow
}

func deviceNow() time.Time {
	if os.Getenv("PROT_E2E") == "1" {
		if path := os.Getenv("PROT_E2E_DEVICE_AUTH_CLOCK_FILE"); path != "" {
			if b, err := os.ReadFile(path); err == nil {
				if sec, err := strconv.ParseInt(strings.TrimSpace(string(b)), 10, 64); err == nil {
					return time.Unix(sec, 0).UTC()
				}
			}
		}
	}
	return time.Now()
}

func publicDeviceProcedure(procedure string) bool {
	return procedure == protectoratev1connect.DeviceServiceStartDeviceAuthProcedure || procedure == protectoratev1connect.DeviceServicePollDeviceAuthProcedure
}

func deviceSourceIP(r *http.Request) netip.Addr {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		host = r.RemoteAddr
	}
	ip, _ := netip.ParseAddr(host)
	ip = ip.Unmap()
	if ip == netip.MustParseAddr("192.168.10.50") {
		parts := strings.Split(r.Header.Get("X-Forwarded-For"), ",")
		if forwarded, err := netip.ParseAddr(strings.TrimSpace(parts[len(parts)-1])); err == nil {
			return forwarded.Unmap()
		}
	}
	return ip
}

func (h *Hub) withDeviceIP(next http.Handler) http.Handler {
	errorWriter := connect.NewErrorWriter()
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := context.WithValue(r.Context(), deviceIPKey{}, deviceSourceIP(r))
		if publicDeviceProcedure(r.URL.Path) {
			ctx = auth.WithNote(ctx)
			if err := h.admitDevice(ctx, r.URL.Path == protectoratev1connect.DeviceServicePollDeviceAuthProcedure); err != nil {
				if r.URL.Path == protectoratev1connect.DeviceServiceStartDeviceAuthProcedure {
					h.authz.Record(ctx, nil, r.URL.Path, "", err, resultCode(err))
				}
				_ = errorWriter.Write(w, r, err)
				return
			}
		}
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

func (h *Hub) admitDevice(ctx context.Context, poll bool) error {
	ip, _ := ctx.Value(deviceIPKey{}).(netip.Addr)
	auth.Annotate(ctx, "", "requesting_ip="+ip.String())
	if !ip.IsValid() || !(ip.IsPrivate() || ip.IsLoopback() || ip.IsLinkLocalUnicast()) {
		return connect.NewError(connect.CodePermissionDenied, errors.New("device authorization is available on the LAN only"))
	}
	now := deviceNow()
	localLimit, globalLimit, kind := 10, 60, "start"
	if poll {
		localLimit, globalLimit, kind = 120, 1200, "poll"
	}
	l := &h.deviceRates
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.windows == nil {
		l.windows = map[string]deviceWindow{}
	}
	for key, window := range l.windows {
		if now.Sub(window.start) >= time.Minute {
			delete(l.windows, key)
		}
	}
	keys := []string{kind + ":global", kind + ":" + ip.String()}
	limits := []int{globalLimit, localLimit}
	for i, key := range keys {
		window := l.windows[key]
		if window.count >= limits[i] {
			return connect.NewError(connect.CodeResourceExhausted, errors.New("device authorization rate limit; retry after one minute"))
		}
	}
	for _, key := range keys {
		window := l.windows[key]
		if window.count == 0 {
			window.start = now
		}
		window.count++
		l.windows[key] = window
	}
	return nil
}

func (h *Hub) StartDeviceAuth(ctx context.Context, req *connect.Request[pb.StartDeviceAuthRequest]) (*connect.Response[pb.StartDeviceAuthResponse], error) {
	ip, _ := ctx.Value(deviceIPKey{}).(netip.Addr)
	auth.Annotate(ctx, "", "requesting_ip="+ip.String())
	r, code, err := h.authz.Store().StartDevice(strings.TrimSpace(req.Msg.Name), strings.TrimSpace(req.Msg.Platform), ip.String(), deviceNow())
	if err != nil {
		status := connect.CodeInvalidArgument
		if errors.Is(err, auth.ErrDeviceCapacity) {
			status = connect.CodeResourceExhausted
		}
		return nil, connect.NewError(status, fmt.Errorf("start device authorization: %w", err))
	}
	h.emit("device_auth.pending", "", nil)
	return connect.NewResponse(&pb.StartDeviceAuthResponse{DeviceCode: code, UserCode: r.UserCode, ExpiresAt: timestamppb.New(r.Expires), Interval: r.Interval}), nil
}

func (h *Hub) PollDeviceAuth(ctx context.Context, req *connect.Request[pb.PollDeviceAuthRequest]) (*connect.Response[pb.PollDeviceAuthResponse], error) {
	if len(req.Msg.DeviceCode) > 128 {
		return nil, invalid("device code is too long")
	}
	status, interval, t, secret, err := h.authz.Store().PollDevice(req.Msg.DeviceCode, deviceNow())
	if err != nil {
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("poll device authorization: %w", err))
	}
	res := &pb.PollDeviceAuthResponse{Status: status, Interval: interval, Secret: secret}
	if secret != "" {
		res.Token = toProtoToken(t)
		h.emit("devices.changed", t.ID, nil)
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) ListDeviceAuth(_ context.Context, _ *connect.Request[pb.ListDeviceAuthRequest]) (*connect.Response[pb.ListDeviceAuthResponse], error) {
	res := &pb.ListDeviceAuthResponse{}
	for _, r := range h.authz.Store().PendingDevices(deviceNow()) {
		res.Requests = append(res.Requests, &pb.DeviceAuthInfo{UserCode: r.UserCode, Name: r.Name, Platform: r.Platform, RequestingIp: r.IP, ExpiresAt: timestamppb.New(r.Expires)})
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) decideDevice(ctx context.Context, code string, approve bool) error {
	p := auth.PrincipalFrom(ctx)
	if p == nil || !(p.Owner || p.Device) {
		return connect.NewError(connect.CodePermissionDenied, errors.New("approval requires the owner or a device token"))
	}
	r, err := h.authz.Store().DecideDevice(code, p.Actor(), approve, deviceNow())
	if err != nil {
		if errors.Is(err, auth.ErrDeviceRequest) {
			return connect.NewError(connect.CodeNotFound, err)
		}
		return connect.NewError(connect.CodeInternal, err)
	}
	auth.Annotate(ctx, r.Name, "platform="+r.Platform+" requesting_ip="+r.IP)
	h.emit("device_auth.changed", "", nil)
	return nil
}

func (h *Hub) ApproveDeviceAuth(ctx context.Context, req *connect.Request[pb.ApproveDeviceAuthRequest]) (*connect.Response[pb.ApproveDeviceAuthResponse], error) {
	if err := h.decideDevice(ctx, req.Msg.UserCode, true); err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.ApproveDeviceAuthResponse{}), nil
}

func (h *Hub) DenyDeviceAuth(ctx context.Context, req *connect.Request[pb.DenyDeviceAuthRequest]) (*connect.Response[pb.DenyDeviceAuthResponse], error) {
	if err := h.decideDevice(ctx, req.Msg.UserCode, false); err != nil {
		return nil, err
	}
	return connect.NewResponse(&pb.DenyDeviceAuthResponse{}), nil
}

func (h *Hub) ListDevices(_ context.Context, _ *connect.Request[pb.ListDevicesRequest]) (*connect.Response[pb.ListDevicesResponse], error) {
	res := &pb.ListDevicesResponse{}
	for _, t := range h.authz.Store().List() {
		if t.IsDevice() {
			res.Devices = append(res.Devices, toProtoToken(t))
		}
	}
	return connect.NewResponse(res), nil
}

func (h *Hub) RevokeDevice(ctx context.Context, req *connect.Request[pb.RevokeDeviceRequest]) (*connect.Response[pb.RevokeDeviceResponse], error) {
	found := false
	for _, t := range h.authz.Store().List() {
		if t.ID == req.Msg.Id && t.IsDevice() {
			found = true
		}
	}
	if !found {
		return nil, connect.NewError(connect.CodeNotFound, errors.New("no such device"))
	}
	t, err := h.authz.Store().Revoke(req.Msg.Id, time.Now())
	if err != nil {
		return nil, connect.NewError(connect.CodeInternal, fmt.Errorf("revoke device: %w", err))
	}
	auth.Annotate(ctx, t.ID, "")
	h.emit("devices.changed", t.ID, nil)
	return connect.NewResponse(&pb.RevokeDeviceResponse{Token: toProtoToken(t)}), nil
}

func (h *Hub) RenameDevice(ctx context.Context, req *connect.Request[pb.RenameDeviceRequest]) (*connect.Response[pb.RenameDeviceResponse], error) {
	t, err := h.authz.Store().RenameDevice(req.Msg.Id, strings.TrimSpace(req.Msg.Name))
	if err != nil {
		code := connect.CodeInvalidArgument
		if errors.Is(err, auth.ErrUnknownToken) {
			code = connect.CodeNotFound
		}
		return nil, connect.NewError(code, fmt.Errorf("rename device: %w", err))
	}
	auth.Annotate(ctx, t.ID, "")
	h.emit("devices.changed", t.ID, nil)
	return connect.NewResponse(&pb.RenameDeviceResponse{Token: toProtoToken(t)}), nil
}

func (h *Hub) sweepDeviceAuth(ctx context.Context) {
	ticker := time.NewTicker(time.Minute)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			if err := h.authz.Store().SweepDevices(deviceNow()); err != nil {
				h.authz.Audit.Write(auth.Entry{Actor: "hub", Procedure: "device_auth.sweep", Outcome: auth.OutcomeError, Detail: err.Error()})
			}
		}
	}
}
