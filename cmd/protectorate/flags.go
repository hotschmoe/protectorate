package main

import (
	"encoding/csv"
	"flag"
	"fmt"
	"io"
	"strconv"
	"strings"
	"time"
)

type flagValue struct {
	flag.Value
	kind string
}

func (v flagValue) Type() string { return v.kind }

type cliFlag struct {
	Name, Shorthand, Usage, DefValue string
	Value                            flagValue
	Changed, Hidden, required        bool
}

type flagSet struct {
	set      *flag.FlagSet
	metadata map[string]*cliFlag
}

func newFlagSet() *flagSet {
	fs := flag.NewFlagSet("", flag.ContinueOnError)
	fs.SetOutput(io.Discard)
	return &flagSet{set: fs, metadata: map[string]*cliFlag{}}
}
func (f *flagSet) Lookup(name string) *cliFlag { return f.metadata[name] }
func (f *flagSet) Changed(name string) bool    { v := f.Lookup(name); return v != nil && v.Changed }
func (f *flagSet) ordered() []*cliFlag {
	var flags []*cliFlag
	f.set.VisitAll(func(v *flag.Flag) { flags = append(flags, f.metadata[v.Name]) })
	return flags
}
func (f *flagSet) VisitAll(visit func(*cliFlag)) {
	for _, v := range f.ordered() {
		visit(v)
	}
}
func (f *flagSet) record(name, short, kind string) {
	v := f.set.Lookup(name)
	f.metadata[name] = &cliFlag{Name: name, Shorthand: short, Usage: v.Usage, DefValue: v.DefValue, Value: flagValue{v.Value, kind}}
}
func (f *flagSet) addExisting(v *cliFlag) {
	if f.Lookup(v.Name) != nil {
		return
	}
	f.set.Var(v.Value.Value, v.Name, v.Usage)
	f.metadata[v.Name] = v
}
func (f *flagSet) StringVar(p *string, name, value, usage string) {
	f.StringVarP(p, name, "", value, usage)
}
func (f *flagSet) StringVarP(p *string, name, short, value, usage string) {
	f.set.StringVar(p, name, value, usage)
	f.record(name, short, "string")
}
func (f *flagSet) BoolVar(p *bool, name string, value bool, usage string) {
	f.BoolVarP(p, name, "", value, usage)
}
func (f *flagSet) BoolVarP(p *bool, name, short string, value bool, usage string) {
	f.set.BoolVar(p, name, value, usage)
	f.record(name, short, "bool")
}
func (f *flagSet) DurationVar(p *time.Duration, name string, value time.Duration, usage string) {
	f.set.DurationVar(p, name, value, usage)
	f.record(name, "", "duration")
}
func (f *flagSet) IntVar(p *int, name string, value int, usage string) {
	f.IntVarP(p, name, "", value, usage)
}
func (f *flagSet) IntVarP(p *int, name, short string, value int, usage string) {
	f.set.IntVar(p, name, value, usage)
	f.record(name, short, "int")
}
func (f *flagSet) Int64Var(p *int64, name string, value int64, usage string) {
	f.set.Int64Var(p, name, value, usage)
	f.record(name, "", "int64")
}
func (f *flagSet) Uint64Var(p *uint64, name string, value uint64, usage string) {
	f.set.Uint64Var(p, name, value, usage)
	f.record(name, "", "uint64")
}
func (f *flagSet) Int(name string, value int, usage string) *int {
	p := new(int)
	f.IntVar(p, name, value, usage)
	return p
}
func (f *flagSet) Uint(name string, value uint, usage string) *uint {
	p := new(uint)
	f.set.UintVar(p, name, value, usage)
	f.record(name, "", "uint")
	return p
}
func (f *flagSet) Int32Var(p *int32, name string, value int32, usage string) {
	*p = value
	f.set.Var(&int32Value{p}, name, usage)
	f.record(name, "", "int32")
}
func (f *flagSet) Uint32Var(p *uint32, name string, value uint32, usage string) {
	*p = value
	f.set.Var(&uint32Value{p}, name, usage)
	f.record(name, "", "uint32")
}

type int32Value struct{ p *int32 }

func (v *int32Value) String() string { return strconv.FormatInt(int64(*v.p), 10) }
func (v *int32Value) Set(raw string) error {
	n, err := strconv.ParseInt(raw, 0, 32)
	if err == nil {
		*v.p = int32(n)
	}
	return err
}

type uint32Value struct{ p *uint32 }

func (v *uint32Value) String() string { return strconv.FormatUint(uint64(*v.p), 10) }
func (v *uint32Value) Set(raw string) error {
	n, err := strconv.ParseUint(raw, 0, 32)
	if err == nil {
		*v.p = uint32(n)
	}
	return err
}

type stringsValue struct {
	p              *[]string
	split, changed bool
}

func (v *stringsValue) String() string {
	var b strings.Builder
	w := csv.NewWriter(&b)
	_ = w.Write(*v.p)
	w.Flush()
	return "[" + strings.TrimSuffix(b.String(), "\n") + "]"
}
func (v *stringsValue) Set(raw string) error {
	values := []string{raw}
	if v.split {
		if raw == "" {
			values = []string{}
		} else {
			var err error
			values, err = csv.NewReader(strings.NewReader(raw)).Read()
			if err != nil {
				return fmt.Errorf("parse comma-separated strings: %w", err)
			}
		}
	}
	if !v.changed {
		*v.p = values
	} else {
		*v.p = append(*v.p, values...)
	}
	v.changed = true
	return nil
}
func (f *flagSet) StringArrayVar(p *[]string, name string, value []string, usage string) {
	f.StringArrayVarP(p, name, "", value, usage)
}
func (f *flagSet) StringArrayVarP(p *[]string, name, short string, value []string, usage string) {
	*p = append([]string(nil), value...)
	f.set.Var(&stringsValue{p: p}, name, usage)
	f.record(name, short, "stringArray")
}
func (f *flagSet) StringSliceVar(p *[]string, name string, value []string, usage string) {
	*p = append([]string(nil), value...)
	f.set.Var(&stringsValue{p: p, split: true}, name, usage)
	f.record(name, "", "stringSlice")
}

func (f *flagSet) parseOne(args []string, apply bool) (int, error) {
	arg := args[0]
	consumed := 1
	if strings.HasPrefix(arg, "--") {
		name, value, hasValue := strings.Cut(arg[2:], "=")
		v := f.Lookup(name)
		if v == nil {
			return consumed, fmt.Errorf("unknown flag: --%s", name)
		}
		if !hasValue && v.Value.kind == "bool" {
			value = "true"
		} else if !hasValue {
			if len(args) < 2 {
				return consumed, fmt.Errorf("flag needs an argument: --%s", name)
			}
			value = args[1]
			consumed++
		}
		if apply {
			return consumed, f.assign(v, value)
		}
		return consumed, nil
	}
	short := arg[1:]
	for len(short) > 0 {
		name := short[:1]
		short = short[1:]
		var v *cliFlag
		f.VisitAll(func(candidate *cliFlag) {
			if candidate.Shorthand == name {
				v = candidate
			}
		})
		if v == nil {
			return consumed, fmt.Errorf("unknown shorthand flag: %q in %s", name, arg)
		}
		value := "true"
		if strings.HasPrefix(short, "=") {
			value = short[1:]
			short = ""
		} else if v.Value.kind != "bool" {
			if short != "" {
				value = short
				short = ""
			} else {
				if len(args) < 2 {
					return consumed, fmt.Errorf("flag needs an argument: -%s", name)
				}
				value = args[1]
				consumed++
			}
		}
		if apply {
			if err := f.assign(v, value); err != nil {
				return consumed, err
			}
		}
	}
	return consumed, nil
}
func (f *flagSet) assign(v *cliFlag, value string) error {
	if err := f.set.Parse([]string{"--" + v.Name + "=" + value}); err != nil {
		return fmt.Errorf("invalid argument %q for --%s: %w", value, v.Name, err)
	}
	v.Changed = true
	return nil
}

func (f *flagSet) writeHelp(w io.Writer) {
	type row struct{ prefix, usage string }
	var rows []row
	width := 0
	f.VisitAll(func(v *cliFlag) {
		if v.Hidden {
			return
		}
		prefix := "      --" + v.Name
		if v.Shorthand != "" {
			prefix = "  -" + v.Shorthand + ", --" + v.Name
		}
		kind := v.Value.kind
		if kind == "stringSlice" {
			kind = "strings"
		}
		if kind == "int64" {
			kind = "int"
		}
		if kind == "uint64" {
			kind = "uint"
		}
		if kind == "bool" {
			kind = ""
		}
		usage := v.Usage
		if start := strings.IndexByte(usage, '`'); start >= 0 {
			if end := strings.IndexByte(usage[start+1:], '`'); end >= 0 {
				end += start + 1
				kind = usage[start+1 : end]
				usage = usage[:start] + kind + usage[end+1:]
			}
		}
		if kind != "" {
			prefix += " " + kind
		}
		def := v.DefValue
		if def != "" && def != "false" && def != "0" && def != "0s" && def != "[]" {
			if v.Value.kind == "string" {
				def = strconv.Quote(def)
			}
			usage += " (default " + def + ")"
		}
		if len(prefix) > width {
			width = len(prefix)
		}
		rows = append(rows, row{prefix, usage})
	})
	for _, row := range rows {
		fmt.Fprintf(w, "%-*s   %s\n", width, row.prefix, row.usage)
	}
}
