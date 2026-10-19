package tailnet

import "tailscale.com/client/local"

func (n *Net) LocalClientForTest() *local.Client { return n.lc }
