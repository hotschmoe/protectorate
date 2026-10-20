package directory

import (
	"strconv"
	"strings"
)

// tomlDoc is the part of a TOML file the manifest readers need: for each table
// (its path joined by tableSep) the raw text of every key's value. It is a
// tolerant reader, not a validator: anything it cannot make sense of is
// skipped.
type tomlDoc map[string]map[string]string

const tableSep = "\x1f"

func tablePath(segs ...string) string { return strings.Join(segs, tableSep) }

func (d tomlDoc) add(path []string, key, raw string) {
	t := tablePath(path...)
	if d[t] == nil {
		d[t] = map[string]string{}
	}
	d[t][key] = raw
}

// tables returns every table path of d split into segments.
func (d tomlDoc) tables() [][]string {
	out := make([][]string, 0, len(d))
	for t := range d {
		out = append(out, strings.Split(t, tableSep))
	}
	return out
}

func parseTOML(src string) tomlDoc {
	doc := tomlDoc{}
	lines := strings.Split(strings.ReplaceAll(src, "\r\n", "\n"), "\n")
	var cur []string
	for i := 0; i < len(lines); i++ {
		line := strings.TrimSpace(stripComment(lines[i]))
		switch {
		case line == "":
		case strings.HasPrefix(line, "[["):
			cur = splitKey(strings.TrimSuffix(strings.TrimPrefix(line, "[["), "]]"))
		case strings.HasPrefix(line, "["):
			cur = splitKey(strings.TrimSuffix(strings.TrimPrefix(line, "["), "]"))
		default:
			eq := indexOutsideQuotes(line, '=')
			if eq < 0 {
				continue
			}
			key := splitKey(strings.TrimSpace(line[:eq]))
			if len(key) == 0 {
				continue
			}
			val, next := readValue(lines, i, strings.TrimSpace(line[eq+1:]))
			i = next
			path := append(append([]string(nil), cur...), key[:len(key)-1]...)
			doc.add(path, key[len(key)-1], val)
		}
	}
	return doc
}

// stripComment removes a trailing # comment that is not inside a string.
func stripComment(s string) string {
	if i := indexOutsideQuotes(s, '#'); i >= 0 {
		return s[:i]
	}
	return s
}

func indexOutsideQuotes(s string, want byte) int {
	var q byte
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case q != 0:
			if c == '\\' && q == '"' {
				i++
			} else if c == q {
				q = 0
			}
		case c == '"' || c == '\'':
			q = c
		case c == want:
			return i
		}
	}
	return -1
}

// splitKey splits a dotted key into its unquoted segments.
func splitKey(s string) []string {
	var out []string
	var b strings.Builder
	var q byte
	flush := func() {
		if t := strings.TrimSpace(b.String()); t != "" {
			out = append(out, t)
		}
		b.Reset()
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case q != 0:
			if c == q {
				q = 0
			} else {
				b.WriteByte(c)
			}
		case c == '"' || c == '\'':
			q = c
		case c == '.':
			flush()
		default:
			b.WriteByte(c)
		}
	}
	flush()
	return out
}

// readValue returns the full text of a value that starts on line i with first
// and may continue over later lines (arrays, inline tables, multi-line
// strings), and the index of the last line it used.
func readValue(lines []string, i int, first string) (string, int) {
	var b strings.Builder
	cur := first
	for {
		b.WriteString(cur)
		if complete(b.String()) || i+1 >= len(lines) {
			return strings.TrimSpace(b.String()), i
		}
		i++
		b.WriteByte('\n')
		cur = stripCommentKeepingStrings(lines[i], b.String())
	}
}

// stripCommentKeepingStrings drops a comment from a continuation line unless
// the value so far is inside a multi-line string.
func stripCommentKeepingStrings(line, soFar string) string {
	if inMultiline(soFar) {
		return line
	}
	return stripComment(line)
}

func inMultiline(s string) bool {
	_, open := scan(s)
	return open
}

func complete(s string) bool {
	depth, open := scan(s)
	return depth <= 0 && !open
}

// scan reports the bracket depth at the end of s and whether it ends inside a
// multi-line string.
func scan(s string) (depth int, inMulti bool) {
	var q byte
	multi := false
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case multi:
			if c == q && strings.HasPrefix(s[i:], strings.Repeat(string(q), 3)) {
				multi, q = false, 0
				i += 2
			} else if c == '\\' && q == '"' {
				i++
			}
		case q != 0:
			if c == '\\' && q == '"' {
				i++
			} else if c == q {
				q = 0
			}
		case c == '"' || c == '\'':
			if strings.HasPrefix(s[i:], strings.Repeat(string(c), 3)) {
				multi, q = true, c
				i += 2
			} else {
				q = c
			}
		case c == '[' || c == '{':
			depth++
		case c == ']' || c == '}':
			depth--
		case c == '#':
			for i < len(s) && s[i] != '\n' {
				i++
			}
		}
	}
	return depth, multi
}

// tomlString decodes a quoted string value.
func tomlString(raw string) (string, bool) {
	raw = strings.TrimSpace(raw)
	switch {
	case strings.HasPrefix(raw, `"""`) && strings.HasSuffix(raw, `"""`) && len(raw) >= 6:
		return strings.TrimPrefix(raw[3:len(raw)-3], "\n"), true
	case strings.HasPrefix(raw, `'''`) && strings.HasSuffix(raw, `'''`) && len(raw) >= 6:
		return strings.TrimPrefix(raw[3:len(raw)-3], "\n"), true
	case len(raw) >= 2 && raw[0] == '\'' && raw[len(raw)-1] == '\'':
		return raw[1 : len(raw)-1], true
	case len(raw) >= 2 && raw[0] == '"' && raw[len(raw)-1] == '"':
		if s, err := strconv.Unquote(raw); err == nil {
			return s, true
		}
		return raw[1 : len(raw)-1], true
	}
	return "", false
}

// splitTop splits the inside of an array or inline table on its top-level
// commas.
func splitTop(inner string) []string {
	var out []string
	var q byte
	depth, start := 0, 0
	for i := 0; i < len(inner); i++ {
		c := inner[i]
		switch {
		case q != 0:
			if c == '\\' && q == '"' {
				i++
			} else if c == q {
				q = 0
			}
		case c == '"' || c == '\'':
			q = c
		case c == '[' || c == '{':
			depth++
		case c == ']' || c == '}':
			depth--
		case c == '#':
			for i < len(inner) && inner[i] != '\n' {
				i++
			}
		case c == ',' && depth == 0:
			out = append(out, inner[start:i])
			start = i + 1
		}
	}
	return append(out, inner[start:])
}

func bracketInner(raw, open, close string) (string, bool) {
	raw = strings.TrimSpace(raw)
	if !strings.HasPrefix(raw, open) || !strings.HasSuffix(raw, close) {
		return "", false
	}
	return raw[1 : len(raw)-1], true
}

// tomlStrings returns the string elements of an array value, skipping others.
func tomlStrings(raw string) []string {
	inner, ok := bracketInner(raw, "[", "]")
	if !ok {
		return nil
	}
	var out []string
	for _, el := range splitTop(inner) {
		if s, ok := tomlString(stripComment(el)); ok {
			out = append(out, s)
		}
	}
	return out
}

// tomlInline returns the key/value pairs of an inline table value.
func tomlInline(raw string) map[string]string {
	inner, ok := bracketInner(raw, "{", "}")
	if !ok {
		return nil
	}
	out := map[string]string{}
	for _, kv := range splitTop(inner) {
		kv = strings.TrimSpace(kv)
		eq := indexOutsideQuotes(kv, '=')
		if eq < 0 {
			continue
		}
		if k := splitKey(kv[:eq]); len(k) == 1 {
			out[k[0]] = strings.TrimSpace(kv[eq+1:])
		}
	}
	return out
}
