package main

import (
	"fmt"
	"io"
	"os"
	"regexp"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

var leaseIDRe = regexp.MustCompile(`^lease-[0-9a-f]{8}$`)

func (o *opts) leaseClient() (protectoratev1connect.LeaseServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewLeaseServiceClient(hc, o.hubURL), nil
}

func leaseCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "lease", Aliases: []string{"leases"}, Short: "Reserve nodes and GPUs"}
	c.AddCommand(leaseAcquireCmd(o), leaseLsCmd(o), leaseRenewCmd(o), leaseReleaseCmd(o))
	return c
}

func defaultHolder() string {
	user := os.Getenv("USER")
	if user == "" {
		user = os.Getenv("USERNAME")
	}
	host, _ := os.Hostname()
	if user == "" {
		return host
	}
	return user + "@" + host
}

func leaseAcquireCmd(o *opts) *cobra.Command {
	var resource, osName, arch, mem, vram, holder string
	var ttl time.Duration
	var allowFamily bool
	c := &cobra.Command{
		Use:   "acquire [--node <name[:gpuN]> | --os <os> --arch <arch> --min-free-mem 8G --gpu-vram 24G] --ttl 30m",
		Short: "Reserve a node, a GPU, or the least-loaded node matching a selector",
		Long: `Print the lease id and the chosen resource. The lease expires unless renewed
before the TTL runs out (lease renew), so a crashed holder frees the hardware.
Selectors pick the least-loaded fresh node; nodes of tier family are skipped
unless --allow-family is set.`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			req := &pb.AcquireLeaseRequest{Holder: holder, Ttl: durationpb.New(ttl), AllowFamily: allowFamily}
			sel := &pb.NodeSelector{Os: osName, Arch: arch}
			var err error
			if sel.MinFreeMemoryBytes, err = parseOptSize(mem); err != nil {
				return err
			}
			if sel.GpuVramBytes, err = parseOptSize(vram); err != nil {
				return err
			}
			if resource != "" {
				if osName != "" || arch != "" || mem != "" || vram != "" {
					return fmt.Errorf("--node and selector flags are exclusive")
				}
				req.Target = &pb.AcquireLeaseRequest_Resource{Resource: resource}
			} else {
				req.Target = &pb.AcquireLeaseRequest_Selector{Selector: sel}
			}
			cl, err := o.leaseClient()
			if err != nil {
				return err
			}
			res, err := cl.AcquireLease(cmd.Context(), connect.NewRequest(req))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeLease(w, res.Msg) })
		},
	}
	f := c.Flags()
	f.StringVar(&resource, "node", "", "node name, <node>:gpu<index> for one GPU, or desk:<node> for the desktop")
	f.StringVar(&osName, "os", "", "required GOOS, e.g. windows")
	f.StringVar(&arch, "arch", "", "required GOARCH, e.g. arm64")
	f.StringVar(&mem, "min-free-mem", "", "least available memory, e.g. 8G")
	f.StringVar(&vram, "gpu-vram", "", "reserve one GPU with at least this much free VRAM, e.g. 24G")
	f.StringVar(&holder, "holder", defaultHolder(), "who holds the lease")
	f.DurationVar(&ttl, "ttl", 30*time.Minute, "lease lifetime without a renewal (at most 24h)")
	f.BoolVar(&allowFamily, "allow-family", false, "allow nodes of tier family")
	return c
}

func parseOptSize(s string) (uint64, error) {
	if s == "" {
		return 0, nil
	}
	return parseSize(s)
}

func writeLease(w io.Writer, l *pb.Lease) {
	fmt.Fprintf(w, "lease\t%s\nresource\t%s\nholder\t%s\nexpires\t%s\n", l.Id, l.Resource, l.Holder, fmtStamp(l.ExpiresAt, "-"))
}

func leaseLsCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "ls",
		Short: "List live leases",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			cl, err := o.leaseClient()
			if err != nil {
				return err
			}
			res, err := cl.ListLeases(cmd.Context(), connect.NewRequest(&pb.ListLeasesRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "ID\tRESOURCE\tHOLDER\tEXPIRES IN")
				for _, l := range res.Msg.Leases {
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", l.Id, l.Resource, l.Holder, time.Until(l.ExpiresAt.AsTime()).Round(time.Second))
				}
			})
		},
	}
}

func leaseRenewCmd(o *opts) *cobra.Command {
	var ttl time.Duration
	c := &cobra.Command{
		Use:   "renew <id>",
		Short: "Extend a lease by its TTL from now",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.leaseClient()
			if err != nil {
				return err
			}
			res, err := cl.RenewLease(cmd.Context(), connect.NewRequest(&pb.RenewLeaseRequest{Id: args[0], Ttl: durationpb.New(ttl)}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeLease(w, res.Msg) })
		},
	}
	c.Flags().DurationVar(&ttl, "ttl", 0, "new lifetime; default the lease's own")
	return c
}

func leaseReleaseCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "release <id>...",
		Short: "Free leases now",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cl, err := o.leaseClient()
			if err != nil {
				return err
			}
			var failed []string
			for _, id := range args {
				res, err := cl.ReleaseLease(cmd.Context(), connect.NewRequest(&pb.ReleaseLeaseRequest{Id: id}))
				if err != nil {
					failed = append(failed, fmt.Sprintf("%s: %v", id, err))
					continue
				}
				if err := o.out(cmd).print(res.Msg, func(w io.Writer) { fmt.Fprintf(w, "released %s\n", id) }); err != nil {
					return err
				}
			}
			if len(failed) > 0 {
				return fmt.Errorf("%s", strings.Join(failed, "; "))
			}
			return nil
		},
	}
}
