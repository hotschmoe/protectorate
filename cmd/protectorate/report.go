package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"os/signal"
	"strings"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func reportStatusLabel(status pb.ReportStatus) string {
	return strings.ToLower(strings.TrimPrefix(status.String(), "REPORT_STATUS_"))
}

func reportCmd(o *opts) *command {
	var status, file string
	c := &command{
		Use:   "report --status complete|partial|blocked [--summary-file F | -]",
		Short: "Report this sleeve's result for its current brief",
		Args:  maximumArgs(1),
		RunE: func(cmd *command, args []string) error {
			value, ok := pb.ReportStatus_value["REPORT_STATUS_"+strings.ToUpper(status)]
			if !ok || value == 0 {
				return errors.New("--status must be complete, partial or blocked")
			}
			if len(args) > 0 && (args[0] != "-" || file != "") {
				return errors.New("pass --summary-file F or - to read stdin")
			}
			var summary []byte
			if file != "" || len(args) > 0 {
				reader := cmd.InOrStdin()
				if file != "" && file != "-" {
					f, err := os.Open(file)
					if err != nil {
						return fmt.Errorf("open report summary: %w", err)
					}
					defer f.Close()
					reader = f
				}
				var err error
				summary, err = io.ReadAll(io.LimitReader(reader, (32<<10)+1))
				if err != nil {
					return fmt.Errorf("read report summary: %w", err)
				}
				if len(summary) > 32<<10 {
					return errors.New("report summary exceeds 32768 bytes")
				}
			}
			cl, err := chatClient(o)
			if err != nil {
				return err
			}
			res, err := cl.Report(cmd.Context(), connect.NewRequest(&pb.ReportRequest{Status: pb.ReportStatus(value), Summary: string(summary)}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "reported %s at chat seq %d\n", status, res.Msg.Result.ChatSeq)
			})
		},
	}
	c.Flags().StringVar(&status, "status", "", "complete, partial or blocked (required)")
	c.Flags().StringVar(&file, "summary-file", "", "UTF-8 summary file, at most 32 KiB; - reads stdin")
	return c
}

func chatWaitCmd(o *opts) *command {
	var until string
	var timeout time.Duration
	c := &command{
		Use:   "wait <sleeve> [sleeve...]",
		Short: "Wait on hub events for all sleeves to become idle or report",
		Args:  minimumArgs(1),
		RunE: func(cmd *command, args []string) error {
			condition := pb.WaitUntil_WAIT_UNTIL_IDLE
			if until == "report" {
				condition = pb.WaitUntil_WAIT_UNTIL_REPORT
			} else if until != "idle" {
				return errors.New("--until must be idle or report")
			}
			if timeout < 0 {
				return errors.New("--timeout must be nonnegative")
			}
			cl, err := chatClient(o)
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt)
			defer stop()
			type outcome struct {
				response *pb.WaitResponse
				err      error
			}
			results := make(chan outcome, len(args))
			seen := map[string]bool{}
			for _, name := range args {
				if seen[name] {
					continue
				}
				seen[name] = true
				go func() {
					res, err := cl.Wait(ctx, connect.NewRequest(&pb.WaitRequest{Sleeve: name, Until: condition, Timeout: durationpb.New(timeout)}))
					if err != nil {
						results <- outcome{err: fmt.Errorf("wait for %s: %w", name, err)}
						return
					}
					results <- outcome{response: res.Msg}
				}()
			}
			for range len(seen) {
				select {
				case <-ctx.Done():
					return ctx.Err()
				case out := <-results:
					if out.err != nil {
						return out.err
					}
					res := out.response
					if err := o.out(cmd).print(res, func(w io.Writer) {
						if res.Result != nil {
							fmt.Fprintf(w, "%s\t%s\n%s\n", res.Sleeve, reportStatusLabel(res.Result.Status), res.Result.Summary)
						} else {
							fmt.Fprintf(w, "%s\tidle\n", res.Sleeve)
						}
					}); err != nil {
						return err
					}
				}
			}
			return nil
		},
	}
	c.Flags().StringVar(&until, "until", "idle", "idle (no pending mail) or report")
	c.Flags().DurationVar(&timeout, "timeout", 0, "maximum wait; 0 waits until interrupted")
	return c
}
