package main

import (
	"context"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"
)

type command struct {
	Use, Short, Long, Example string
	Aliases                   []string
	Hidden                    bool
	Args                      func(*command, []string) error
	RunE                      func(*command, []string) error
	Run                       func(*command, []string)
	parent                    *command
	children                  []*command
	flags, persistent         *flagSet
	ctx                       context.Context
	dash                      int
	omitFlagsInUsage          bool
	rawArguments              bool
}

func (c *command) Name() string { name, _, _ := strings.Cut(c.Use, " "); return name }
func (c *command) CommandPath() string {
	if c.parent == nil {
		return c.Name()
	}
	return c.parent.CommandPath() + " " + c.Name()
}
func (c *command) Root() *command {
	for c.parent != nil {
		c = c.parent
	}
	return c
}
func (c *command) Runnable() bool           { return c.Run != nil || c.RunE != nil }
func (c *command) Context() context.Context { return c.ctx }
func (c *command) OutOrStdout() io.Writer   { return os.Stdout }
func (c *command) ErrOrStderr() io.Writer   { return os.Stderr }
func (c *command) InOrStdin() io.Reader     { return os.Stdin }
func (c *command) ArgsLenAtDash() int       { return c.dash }
func (c *command) Flags() *flagSet {
	if c.flags == nil {
		c.flags = newFlagSet()
	}
	return c.flags
}
func (c *command) PersistentFlags() *flagSet {
	if c.persistent == nil {
		c.persistent = newFlagSet()
	}
	return c.persistent
}
func (c *command) NonInheritedFlags() *flagSet { return c.Flags() }
func (c *command) AddCommand(children ...*command) {
	for _, child := range children {
		child.parent = c
		c.children = append(c.children, child)
	}
}
func (c *command) Commands() []*command {
	children := append([]*command(nil), c.children...)
	sort.Slice(children, func(i, j int) bool { return children[i].Name() < children[j].Name() })
	return children
}
func (c *command) MarkFlagRequired(name string) error {
	f := c.Flags().Lookup(name)
	if f == nil {
		return fmt.Errorf("unknown required flag %q", name)
	}
	f.required = true
	return nil
}
func (c *command) child(name string) *command {
	for _, child := range c.children {
		if child.Name() == name {
			return child
		}
		for _, alias := range child.Aliases {
			if alias == name {
				return child
			}
		}
	}
	return nil
}
func (c *command) visibleFlags() *flagSet {
	f := newFlagSet()
	for cur := c; cur != nil; cur = cur.parent {
		if cur.persistent != nil {
			cur.persistent.VisitAll(func(v *cliFlag) {
				if f.Lookup(v.Name) == nil {
					f.addExisting(v)
				}
			})
		}
	}
	c.Flags().VisitAll(func(v *cliFlag) { f.addExisting(v) })
	return f
}

func (c *command) ExecuteContext(ctx context.Context) error {
	c.addBuiltins()
	return c.execute(ctx, os.Args[1:])
}

func (c *command) execute(ctx context.Context, args []string) error {
	c.ctx = ctx
	c.dash = -1
	if c.rawArguments {
		return c.RunE(c, args)
	}
	help := false
	c.Flags().BoolVarP(&help, "help", "h", false, "help for "+c.Name())
	for i := 0; i < len(args); i++ {
		arg := args[i]
		if arg == "--" {
			break
		}
		if strings.HasPrefix(arg, "-") && arg != "-" {
			n, err := c.visibleFlags().parseOne(args[i:], false)
			if err != nil {
				c.usage(c.ErrOrStderr())
				return err
			}
			i += n - 1
			continue
		}
		if child := c.child(arg); child != nil {
			childArgs := append(append([]string(nil), args[:i]...), args[i+1:]...)
			return child.execute(ctx, childArgs)
		}
		break
	}
	positional := []string{}
	for i := 0; i < len(args); i++ {
		arg := args[i]
		if arg == "--" {
			c.dash = len(positional)
			positional = append(positional, args[i+1:]...)
			break
		}
		if strings.HasPrefix(arg, "-") && arg != "-" {
			consumed, err := c.visibleFlags().parseOne(args[i:], true)
			if err != nil {
				c.usage(c.ErrOrStderr())
				return err
			}
			i += consumed - 1
			continue
		}
		if len(positional) == 0 {
			if len(c.children) > 0 && !c.Runnable() {
				c.usage(c.ErrOrStderr())
				return fmt.Errorf("unknown command %q for %q", arg, c.CommandPath())
			}
		}
		positional = append(positional, arg)
	}
	if help {
		return c.Help()
	}
	if c.Name() == "help" {
		target := c.Root()
		for _, part := range positional {
			next := target.child(part)
			if next == nil {
				c.usage(c.ErrOrStderr())
				return fmt.Errorf("unknown help topic %q", strings.Join(positional, " "))
			}
			target = next
		}
		return target.Help()
	}
	if !c.Runnable() {
		return c.Help()
	}
	if c.Args != nil {
		if err := c.Args(c, positional); err != nil {
			c.usage(c.ErrOrStderr())
			return err
		}
	}
	var missing []string
	c.visibleFlags().VisitAll(func(f *cliFlag) {
		if f.required && !f.Changed {
			missing = append(missing, fmt.Sprintf("%q", f.Name))
		}
	})
	if len(missing) > 0 {
		c.usage(c.ErrOrStderr())
		return fmt.Errorf("required flag(s) %s not set", strings.Join(missing, ", "))
	}
	if c.RunE != nil {
		return c.RunE(c, positional)
	}
	c.Run(c, positional)
	return nil
}

func (c *command) Help() error {
	w := c.OutOrStdout()
	description := c.Long
	if description == "" {
		description = c.Short
	}
	if description != "" {
		fmt.Fprintln(w, description)
		fmt.Fprintln(w)
	}
	c.usage(w)
	return nil
}

func (c *command) usage(w io.Writer) {
	fmt.Fprintln(w, "Usage:")
	if c.Runnable() {
		use := c.Use
		if c.parent != nil {
			use = c.parent.CommandPath() + " " + use
		}
		if len(c.visibleFlags().ordered()) > 0 && !strings.Contains(use, "[flags]") && !c.omitFlagsInUsage {
			use += " [flags]"
		}
		fmt.Fprintln(w, "  "+use)
	}
	if len(c.children) > 0 {
		fmt.Fprintln(w, "  "+c.CommandPath()+" [command]")
	}
	if c.Example != "" {
		fmt.Fprintln(w, "\nExamples:")
		fmt.Fprintln(w, c.Example)
	}
	if len(c.Aliases) > 0 {
		fmt.Fprintln(w, "\nAliases:")
		fmt.Fprintln(w, "  "+strings.Join(append([]string{c.Name()}, c.Aliases...), ", "))
	}
	if len(c.children) > 0 {
		fmt.Fprintln(w, "\nAvailable Commands:")
		width := 11
		for _, child := range c.children {
			if !child.Hidden && len(child.Name()) > width {
				width = len(child.Name())
			}
		}
		for _, child := range c.Commands() {
			if !child.Hidden {
				fmt.Fprintf(w, "  %-*s %s\n", width, child.Name(), child.Short)
			}
		}
	}
	local := newFlagSet()
	c.Flags().VisitAll(local.addExisting)
	if c.parent == nil && c.persistent != nil {
		c.persistent.VisitAll(local.addExisting)
	}
	help := false
	if local.Lookup("help") == nil {
		local.BoolVarP(&help, "help", "h", false, "help for "+c.Name())
	}
	fmt.Fprintln(w, "\nFlags:")
	local.writeHelp(w)
	inherited := newFlagSet()
	for parent := c.parent; parent != nil; parent = parent.parent {
		if parent.persistent != nil {
			parent.persistent.VisitAll(func(f *cliFlag) {
				if local.Lookup(f.Name) == nil && inherited.Lookup(f.Name) == nil {
					inherited.addExisting(f)
				}
			})
		}
	}
	if len(inherited.ordered()) > 0 {
		fmt.Fprintln(w, "\nGlobal Flags:")
		inherited.writeHelp(w)
	}
	if len(c.children) > 0 {
		fmt.Fprintf(w, "\nUse %q for more information about a command.\n", c.CommandPath()+" [command] --help")
	}
}

func noArgs(c *command, args []string) error {
	if len(args) > 0 {
		return fmt.Errorf("unknown command %q for %q", args[0], c.CommandPath())
	}
	return nil
}
func arbitraryArgs(*command, []string) error { return nil }
func exactArgs(n int) func(*command, []string) error {
	return func(_ *command, args []string) error {
		if len(args) != n {
			return fmt.Errorf("accepts %d arg(s), received %d", n, len(args))
		}
		return nil
	}
}
func minimumArgs(n int) func(*command, []string) error {
	return func(_ *command, args []string) error {
		if len(args) < n {
			return fmt.Errorf("requires at least %d arg(s), only received %d", n, len(args))
		}
		return nil
	}
}
func maximumArgs(n int) func(*command, []string) error {
	return func(_ *command, args []string) error {
		if len(args) > n {
			return fmt.Errorf("accepts at most %d arg(s), received %d", n, len(args))
		}
		return nil
	}
}
func rangeArgs(min, max int) func(*command, []string) error {
	return func(_ *command, args []string) error {
		if len(args) < min || len(args) > max {
			return fmt.Errorf("accepts between %d and %d arg(s), received %d", min, max, len(args))
		}
		return nil
	}
}
