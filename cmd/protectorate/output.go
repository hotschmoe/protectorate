package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"text/tabwriter"

	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
)

// out renders proto messages either as stable one-line proto-JSON or as
// aligned text. Every command prints through it.
type out struct {
	w    io.Writer
	json bool
}

// print writes m as compact proto-JSON in json mode, otherwise calls human
// with a tabwriter (tab-separated columns are aligned).
func (o out) print(m proto.Message, human func(w io.Writer)) error {
	if o.json {
		b, err := protoJSON(m)
		if err != nil {
			return err
		}
		_, err = fmt.Fprintf(o.w, "%s\n", b)
		return err
	}
	tw := tabwriter.NewWriter(o.w, 0, 0, 2, ' ', 0)
	human(tw)
	return tw.Flush()
}

// protoJSON is protojson made deterministic: protojson deliberately
// randomizes whitespace, so the output is compacted.
func protoJSON(m proto.Message) ([]byte, error) {
	raw, err := protojson.MarshalOptions{EmitUnpopulated: false}.Marshal(m)
	if err != nil {
		return nil, err
	}
	var buf bytes.Buffer
	if err := json.Compact(&buf, raw); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}
