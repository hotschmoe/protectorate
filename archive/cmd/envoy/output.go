package main

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"
)

// OutputWriter handles JSON and table output formatting
type OutputWriter struct {
	json bool
	w    io.Writer
}

// NewOutputWriter creates a new output writer
func NewOutputWriter(jsonMode bool, w io.Writer) *OutputWriter {
	return &OutputWriter{
		json: jsonMode,
		w:    w,
	}
}

// Write outputs data as JSON or table format
// tableFunc returns (headers, rows) for table formatting
func (o *OutputWriter) Write(v interface{}, tableFunc func() ([]string, [][]string)) error {
	if o.json {
		enc := json.NewEncoder(o.w)
		enc.SetIndent("", "  ")
		return enc.Encode(v)
	}

	if tableFunc == nil {
		return fmt.Errorf("no table formatter provided")
	}

	headers, rows := tableFunc()
	o.writeTable(headers, rows)
	return nil
}

// writeTable prints a formatted table
func (o *OutputWriter) writeTable(headers []string, rows [][]string) {
	if len(headers) == 0 {
		return
	}

	widths := make([]int, len(headers))
	for i, h := range headers {
		widths[i] = len(h)
	}
	for _, row := range rows {
		for i, cell := range row {
			if i < len(widths) && len(cell) > widths[i] {
				widths[i] = len(cell)
			}
		}
	}

	var sb strings.Builder

	for i, h := range headers {
		if i > 0 {
			sb.WriteString("  ")
		}
		sb.WriteString(padRight(h, widths[i]))
	}
	sb.WriteString("\n")

	for i := range headers {
		if i > 0 {
			sb.WriteString("  ")
		}
		sb.WriteString(strings.Repeat("-", widths[i]))
	}
	sb.WriteString("\n")

	for _, row := range rows {
		for i, cell := range row {
			if i > 0 {
				sb.WriteString("  ")
			}
			if i < len(widths) {
				sb.WriteString(padRight(cell, widths[i]))
			}
		}
		sb.WriteString("\n")
	}

	fmt.Fprint(o.w, sb.String())
}

// WriteMessage outputs a simple message (not affected by JSON mode for status messages)
func (o *OutputWriter) WriteMessage(format string, args ...interface{}) {
	fmt.Fprintf(o.w, format+"\n", args...)
}

// WriteError outputs an error message
func (o *OutputWriter) WriteError(format string, args ...interface{}) {
	fmt.Fprintf(o.w, "Error: "+format+"\n", args...)
}

func padRight(s string, width int) string {
	if len(s) >= width {
		return s
	}
	return s + strings.Repeat(" ", width-len(s))
}
