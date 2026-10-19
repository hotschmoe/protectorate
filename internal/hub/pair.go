package hub

import (
	"fmt"
	"io"
	"net/url"
	"strings"

	"rsc.io/qr"
)

// PairLinks are the two forms of a pairing link. The web link keeps the
// credential in the URL fragment so browsers never send it to a server.
type PairLinks struct {
	Web    string
	Scheme string
}

// NewPairLinks builds pairing links for a hub. The credential is the hub
// token today; a scoped, expiring device token can replace it without
// changing the link format.
func NewPairLinks(baseURL, name, token string) (PairLinks, error) {
	u, err := url.Parse(strings.TrimRight(baseURL, "/"))
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		return PairLinks{}, fmt.Errorf("pair url must be http:// or https://, got %q", baseURL)
	}
	base := u.String()
	web := url.Values{"name": {name}, "token": {token}}
	app := url.Values{"url": {base}, "name": {name}, "token": {token}}
	return PairLinks{
		Web:    base + "/#/pair?" + web.Encode(),
		Scheme: "protectorate://pair?" + app.Encode(),
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
