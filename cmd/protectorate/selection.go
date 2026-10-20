package main

import (
	"fmt"
	"io"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

func selectionFlags(c *command, s *pb.ModelSelection) {
	c.Flags().StringVar(&s.Model, "model", "", "native model ID (Codex chat; empty uses native default)")
	c.Flags().StringVar(&s.Effort, "effort", "", "native reasoning effort, validated against model catalog")
}
func optionalSelection(s *pb.ModelSelection) *pb.ModelSelection {
	if s.Model == "" && s.Effort == "" {
		return nil
	}
	return s
}
func printSelection(w io.Writer, c *pb.ChatStatus) {
	value := func(s, fallback string) string {
		if s == "" {
			return fallback
		}
		return s
	}
	fmt.Fprintf(w, "model\t%s\neffort\t%s\nrequested model\t%s\nrequested effort\t%s\n", value(c.GetModel(), "unknown"), value(c.GetReasoningEffort(), "unknown"), value(c.GetSettings().GetRequested().GetModel(), "native default"), value(c.GetSettings().GetRequested().GetEffort(), "native default"))
}
func chatModelsCmd(o *opts) *command {
	var harness string
	c := &command{Use: "models [sleeve]", Short: "List native model and effort capabilities (no inference)", Args: maximumArgs(1), RunE: func(cmd *command, args []string) error {
		cl, err := chatClient(o)
		if err != nil {
			return err
		}
		r := &pb.ListModelsRequest{Harness: harness}
		if len(args) > 0 {
			r.Sleeve = args[0]
		}
		res, err := cl.ListModels(cmd.Context(), connect.NewRequest(r))
		if err != nil {
			return err
		}
		return o.out(cmd).print(res.Msg, func(w io.Writer) {
			if !res.Msg.Supported {
				fmt.Fprintln(w, res.Msg.Reason)
				return
			}
			for _, m := range res.Msg.Models {
				fmt.Fprintf(w, "%s\t%s\tefforts=%v\tdefault effort=%s\n", m.Model, m.Name, m.Efforts, m.DefaultEffort)
			}
		})
	}}
	c.Flags().StringVar(&harness, "harness", "codex", "harness catalog when no sleeve is supplied")
	return c
}
func chatSettingsCmd(o *opts) *command {
	sel := &pb.ModelSelection{}
	var defaults bool
	c := &command{Use: "settings <sleeve>", Short: "Show or replace model/effort selection between turns", Long: "Show settings without flags. --model and --effort replace the full requested selection; an omitted field uses its native default. --native-defaults clears both. Codex updates the same session without restarting; other drivers reject changes.", Args: exactArgs(1), RunE: func(cmd *command, args []string) error {
		if defaults && (cmd.Flags().Changed("model") || cmd.Flags().Changed("effort")) {
			return fmt.Errorf("--native-defaults cannot be combined with --model or --effort")
		}
		if !defaults && !cmd.Flags().Changed("model") && !cmd.Flags().Changed("effort") {
			cl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			r, err := cl.GetSleeve(cmd.Context(), connect.NewRequest(&pb.GetSleeveRequest{Name: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(r.Msg.Sleeve.Chat, func(w io.Writer) { printSelection(w, r.Msg.Sleeve.Chat) })
		}
		cl, err := chatClient(o)
		if err != nil {
			return err
		}
		r, err := cl.SetSelection(cmd.Context(), connect.NewRequest(&pb.SetSelectionRequest{Sleeve: args[0], Selection: sel}))
		if err != nil {
			return err
		}
		return o.out(cmd).print(r.Msg, func(w io.Writer) { printSelection(w, r.Msg.Chat) })
	}}
	selectionFlags(c, sel)
	c.Flags().BoolVar(&defaults, "native-defaults", false, "restore the session's captured native model and effort defaults")
	return c
}
