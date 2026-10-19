package hub

import (
	"bytes"
	"net/url"
	"strings"
	"testing"
)

func TestNewPairLinks(t *testing.T) {
	l, err := NewPairLinks("https://hub.example:7470/", "my hub&1", "t0k/en")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(l.Web, "https://hub.example:7470/#/pair?") {
		t.Fatalf("web link: %s", l.Web)
	}
	frag := l.Web[strings.Index(l.Web, "?")+1:]
	q, err := url.ParseQuery(frag)
	if err != nil || q.Get("name") != "my hub&1" || q.Get("token") != "t0k/en" {
		t.Fatalf("web query %v err %v", q, err)
	}
	u, err := url.Parse(l.Scheme)
	if err != nil || u.Scheme != "protectorate" || u.Host != "pair" {
		t.Fatalf("scheme link: %s", l.Scheme)
	}
	if u.Query().Get("url") != "https://hub.example:7470" || u.Query().Get("token") != "t0k/en" {
		t.Fatalf("scheme query: %v", u.Query())
	}
}

func TestNewPairLinksRejectsBadURL(t *testing.T) {
	for _, bad := range []string{"", "hub.example", "ftp://x", "http://"} {
		if _, err := NewPairLinks(bad, "n", "t"); err == nil {
			t.Errorf("%q accepted", bad)
		}
	}
}

func TestWriteQR(t *testing.T) {
	var b bytes.Buffer
	if err := WriteQR(&b, "protectorate://pair?url=http://x&token=y"); err != nil {
		t.Fatal(err)
	}
	if lines := strings.Count(b.String(), "\n"); lines < 10 {
		t.Fatalf("too few rows: %d", lines)
	}
}
