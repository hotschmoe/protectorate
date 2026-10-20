package main

import (
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"strings"
	"syscall"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/hub"
)

func deviceTokenPath(home, hubURL string) (string, error) {
	u, err := url.Parse(strings.TrimRight(hubURL, "/"))
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return "", fmt.Errorf("hub URL must be http:// or https:// without credentials, query or fragment")
	}
	u.Host = strings.ToLower(u.Host)
	if home == "" {
		home, err = hub.Home()
		if err != nil {
			return "", fmt.Errorf("resolve device credential home: %w", err)
		}
	}
	return filepath.Join(home, "clients", fmt.Sprintf("%x.token", sha256.Sum256([]byte(u.String())))), nil
}

func (o *opts) savedDeviceToken() (string, error) {
	path, err := deviceTokenPath("", o.hubURL)
	if err != nil {
		return "", err
	}
	b, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return "", nil
	}
	if err != nil {
		return "", fmt.Errorf("read saved device credential: %w", err)
	}
	if secret := strings.TrimSpace(string(b)); secret != "" {
		return secret, nil
	}
	return "", errors.New("saved device credential is empty; run protectorate login --hub URL again")
}

func runDeviceLogin(cmd *command, o *opts, home, name, platform string) error {
	path, err := deviceTokenPath(home, o.hubURL)
	if err != nil {
		return err
	}
	if name == "" {
		name, err = os.Hostname()
		if err != nil {
			return fmt.Errorf("resolve device name: %w", err)
		}
	}
	if platform == "" {
		platform = runtime.GOOS + "/" + runtime.GOARCH
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return fmt.Errorf("prepare device credential directory: %w", err)
	}
	ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	client := protectoratev1connect.NewDeviceServiceClient(&http.Client{
		Timeout: 20 * time.Second,
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}, o.hubURL)
	start, err := client.StartDeviceAuth(ctx, connect.NewRequest(&pb.StartDeviceAuthRequest{Name: name, Platform: platform}))
	if err != nil {
		return fmt.Errorf("start device authorization at %s: %w", o.hubURL, err)
	}
	if err := o.out(cmd).print(start.Msg, func(w io.Writer) {
		fmt.Fprintf(w, "\nCode: %s\n\nOn a trusted device, approve only if this code matches.\nOn the hub: protectorate devices approve %s\n\nWaiting for approval until %s...\n", start.Msg.UserCode, start.Msg.UserCode, fmtStamp(start.Msg.ExpiresAt, "expiry"))
	}); err != nil {
		return fmt.Errorf("display device authorization: %w", err)
	}
	if start.Msg.ExpiresAt != nil {
		var cancel context.CancelFunc
		ctx, cancel = context.WithDeadline(ctx, start.Msg.ExpiresAt.AsTime())
		defer cancel()
	}
	interval := max(start.Msg.Interval, 1)
	for {
		timer := time.NewTimer(time.Duration(interval) * time.Second)
		select {
		case <-ctx.Done():
			timer.Stop()
			if errors.Is(ctx.Err(), context.DeadlineExceeded) {
				return errors.New("device authorization expired; run login again to retry")
			}
			return fmt.Errorf("device authorization canceled: %w", ctx.Err())
		case <-timer.C:
		}
		res, err := client.PollDeviceAuth(ctx, connect.NewRequest(&pb.PollDeviceAuthRequest{DeviceCode: start.Msg.DeviceCode}))
		if err != nil {
			if connect.CodeOf(err) == connect.CodeResourceExhausted {
				interval += 5
				continue
			}
			return fmt.Errorf("poll device authorization: %w", err)
		}
		switch res.Msg.Status {
		case "pending":
			interval = max(interval, res.Msg.Interval)
			continue
		case "slow_down":
			interval = max(interval+5, res.Msg.Interval)
			continue
		case "approved":
			if res.Msg.Secret == "" {
				return errors.New("hub approved this device without returning its credential; run login again")
			}
			if err := fsutil.WriteFile(path, []byte(res.Msg.Secret+"\n"), 0o600); err != nil {
				return fmt.Errorf("save device credential to %s: %w", path, err)
			}
			res.Msg.Secret = ""
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "Connected to %s. Credential saved in %s.\n", o.hubURL, path)
			})
		case "denied":
			return errors.New("device authorization denied by a trusted device")
		case "expired":
			return errors.New("device authorization expired; run login again to retry")
		case "consumed":
			return errors.New("device credential was already delivered; run login again")
		default:
			return fmt.Errorf("unexpected device authorization status %q", res.Msg.Status)
		}
	}
}

func (o *opts) deviceClient() (protectoratev1connect.DeviceServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, fmt.Errorf("resolve trusted device credential: %w", err)
	}
	return protectoratev1connect.NewDeviceServiceClient(hc, o.hubURL), nil
}

func devicesCmd(o *opts) *command {
	c := &command{Use: "devices", Short: "List, approve, rename and revoke trusted devices"}
	c.AddCommand(&command{
		Use: "ls", Short: "List trusted devices", Args: noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := o.deviceClient()
			if err != nil {
				return err
			}
			res, err := cl.ListDevices(cmd.Context(), connect.NewRequest(&pb.ListDevicesRequest{}))
			if err != nil {
				return fmt.Errorf("list devices: %w", err)
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "ID\tNAME\tPLATFORM\tSTATE\tLAST USED\tAPPROVED\tAPPROVER")
				for _, t := range res.Msg.Devices {
					state := "active"
					if t.RevokedAt != nil {
						state = "revoked"
					} else if t.ExpiresAt != nil && !time.Now().Before(t.ExpiresAt.AsTime()) {
						state = "expired"
					}
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\t%s\n", t.Id, t.Name, orDash(t.Platform), state, fmtStamp(t.LastUsedAt, "never"), fmtStamp(t.ApprovedAt, "unknown"), orDash(t.ApprovedBy))
				}
			})
		},
	}, &command{
		Use: "pending", Short: "List requests awaiting approval", Args: noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := o.deviceClient()
			if err != nil {
				return err
			}
			res, err := cl.ListDeviceAuth(cmd.Context(), connect.NewRequest(&pb.ListDeviceAuthRequest{}))
			if err != nil {
				return fmt.Errorf("list pending device requests: %w", err)
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "CODE\tNAME\tPLATFORM\tREQUESTING IP\tEXPIRES")
				for _, r := range res.Msg.Requests {
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\n", r.UserCode, r.Name, r.Platform, r.RequestingIp, fmtStamp(r.ExpiresAt, "-"))
				}
			})
		},
	}, &command{
		Use: "approve <code>", Short: "Approve a request after matching its code on the new device", Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.deviceClient()
			if err != nil {
				return err
			}
			res, err := cl.ApproveDeviceAuth(cmd.Context(), connect.NewRequest(&pb.ApproveDeviceAuthRequest{UserCode: args[0]}))
			if err != nil {
				return fmt.Errorf("approve device request: %w", err)
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "approved %s\n", args[0]) })
		},
	}, &command{
		Use: "deny <code>", Short: "Deny a device request", Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.deviceClient()
			if err != nil {
				return err
			}
			res, err := cl.DenyDeviceAuth(cmd.Context(), connect.NewRequest(&pb.DenyDeviceAuthRequest{UserCode: args[0]}))
			if err != nil {
				return fmt.Errorf("deny device request: %w", err)
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "denied %s\n", args[0]) })
		},
	}, &command{
		Use: "revoke <id>", Short: "Revoke a device and disconnect its streams immediately", Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.deviceClient()
			if err != nil {
				return err
			}
			res, err := cl.RevokeDevice(cmd.Context(), connect.NewRequest(&pb.RevokeDeviceRequest{Id: args[0]}))
			if err != nil {
				return fmt.Errorf("revoke device: %w", err)
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "revoked %s\n", args[0]) })
		},
	}, &command{
		Use: "rename <id> <name>", Short: "Rename a trusted device", Args: exactArgs(2),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.deviceClient()
			if err != nil {
				return err
			}
			res, err := cl.RenameDevice(cmd.Context(), connect.NewRequest(&pb.RenameDeviceRequest{Id: args[0], Name: args[1]}))
			if err != nil {
				return fmt.Errorf("rename device: %w", err)
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "renamed %s to %s\n", args[0], args[1]) })
		},
	})
	return c
}
