package hub

import (
	"fmt"
	"io"
	"net/url"
	"strings"

	"rsc.io/qr"
)

// PairLinks are the two forms of a connection link, containing only the hub URL.
type PairLinks struct {
	Web    string
	Scheme string
}

func NewPairLinks(baseURL string) (PairLinks, error) {
	u, err := url.Parse(strings.TrimRight(baseURL, "/"))
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return PairLinks{}, fmt.Errorf("pair URL must be http:// or https:// without credentials, query or fragment")
	}
	base := u.String()
	query := url.Values{"url": {base}}
	return PairLinks{
		Web:    base + "/#/pair?" + query.Encode(),
		Scheme: "protectorate://pair?" + query.Encode(),
	}, nil
}

// WriteQR renders text as a QR code using ANSI-colored half blocks, two
// modules per terminal row, on a white quiet zone.
func WriteQR(w io.Writer, text string) error {
	code, err := qr.Encode(text, qr.M)
	if err != nil {
		return fmt.Errorf("encode qr: %w", err)
	}
	const quiet = 2
	dark := func(x, y int) bool {
		return x >= 0 && y >= 0 && x < code.Size && y < code.Size && code.Black(x, y)
	}
	color := func(black bool, fg bool) int {
		switch {
		case fg && black:
			return 30
		case fg:
			return 37
		case black:
			return 40
		}
		return 47
	}
	for y := -quiet; y < code.Size+quiet; y += 2 {
		var b strings.Builder
		for x := -quiet; x < code.Size+quiet; x++ {
			fmt.Fprintf(&b, "\x1b[%d;%dm▀", color(dark(x, y), true), color(dark(x, y+1), false))
		}
		b.WriteString("\x1b[0m\n")
		if _, err := io.WriteString(w, b.String()); err != nil {
			return err
		}
	}
	return nil
}
