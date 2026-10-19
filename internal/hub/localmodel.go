package hub

import (
	"net"
	"net/netip"
	"net/url"
	"strings"
)

var cgnat = netip.MustParsePrefix("100.64.0.0/10")

// servedLocally reports whether a model endpoint is a server of the user's
// own: loopback, private, link-local, tailnet or CGNAT addresses, single-label
// and .local/.ts.net names, or a host in nodeHosts (inventory addresses and
// names), which include the hub's --local-model-host list. An empty base URL is a named hosted provider and is never local.
func servedLocally(baseURL string, nodeHosts []string) bool {
	if baseURL == "" {
		return false
	}
	u, err := url.Parse(baseURL)
	if err != nil || u.Hostname() == "" {
		return false
	}
	host := strings.TrimSuffix(strings.ToLower(u.Hostname()), ".")
	for _, n := range nodeHosts {
		if host == strings.ToLower(n) {
			return true
		}
	}
	if ip, err := netip.ParseAddr(host); err == nil {
		ip = ip.Unmap()
		return ip.IsLoopback() || ip.IsPrivate() || ip.IsLinkLocalUnicast() || ip.IsUnspecified() || cgnat.Contains(ip)
	}
	return !strings.Contains(host, ".") ||
		strings.HasSuffix(host, ".local") || strings.HasSuffix(host, ".localhost") || strings.HasSuffix(host, ".ts.net")
}

// nodeHostnames lists the configured local model hosts and the inventory's
// nodes, as bare names and addresses with any port removed.
func (h *Hub) nodeHostnames() []string {
	out := append([]string(nil), h.LocalModelHosts...)
	if h.nodes == nil {
		return out
	}
	for _, n := range h.nodes.inv.List() {
		out = append(out, n.GetName())
		addr := n.GetAddress()
		if host, _, err := net.SplitHostPort(addr); err == nil {
			addr = host
		}
		out = append(out, strings.Trim(addr, "[]"))
	}
	return out
}
