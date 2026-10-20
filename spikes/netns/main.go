// Spike: rootless per-sleeve network namespace with a userspace NAT in the hub.
//
//	netns run [-forward 127.0.0.1:18000=8000] [-allow host,host] -- <bwrap args... -- cmd>
//
// The hub re-execs itself in a fresh user+net namespace ("netchild"). The child
// creates a TAP, passes the fd back over a unix socket, then execs the command
// (bwrap without --unshare-net), so the sleeve shares that namespace. The hub
// runs a gvisor netstack on the TAP fd and forwards TCP/UDP to host sockets.
package main

import (
	"flag"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"golang.org/x/sys/unix"
)

const (
	gwIP     = "10.0.2.2"
	dnsIP    = "10.0.2.3"
	sleeveIP = "10.0.2.100"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "netchild" {
		netChild(os.Args[2:])
		return
	}
	if len(os.Args) < 2 || os.Args[1] != "run" {
		fmt.Fprintln(os.Stderr, "usage: netns run [-forward laddr=port] [-allow h1,h2] -- cmd...")
		os.Exit(2)
	}
	fs := flag.NewFlagSet("run", flag.ExitOnError)
	forward := fs.String("forward", "", "laddr=sleeveport: hub-side port forward into the sleeve")
	allow := fs.String("allow", "", "comma separated DNS names the sleeve may reach (empty: all)")
	gwLoop := fs.Bool("gw-loopback", true, "map 10.0.2.2 to host 127.0.0.1")
	gwPorts := fs.String("gw-ports", "", "comma separated host loopback ports reachable via the gateway (empty: all)")
	fs.Parse(os.Args[2:])
	if err := run(fs.Args(), *forward, *allow, *gwLoop, splitPorts(*gwPorts)); err != nil {
		fmt.Fprintln(os.Stderr, "netns:", err)
		os.Exit(1)
	}
}

func run(argv []string, forward, allow string, gwLoop bool, gwPorts []uint16) error {
	if len(argv) == 0 {
		return fmt.Errorf("no command")
	}
	t0 := time.Now()
	pair, err := unix.Socketpair(unix.AF_UNIX, unix.SOCK_STREAM|unix.SOCK_CLOEXEC, 0)
	if err != nil {
		return fmt.Errorf("socketpair: %w", err)
	}
	hubEnd := os.NewFile(uintptr(pair[0]), "hub")
	childEnd := os.NewFile(uintptr(pair[1]), "child")

	cmd := exec.Command("/proc/self/exe", append([]string{"netchild"}, argv...)...)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	cmd.ExtraFiles = []*os.File{childEnd}
	uid, gid := os.Getuid(), os.Getgid()
	cmd.SysProcAttr = &syscall.SysProcAttr{
		Cloneflags:                 syscall.CLONE_NEWUSER | syscall.CLONE_NEWNET,
		UidMappings:                []syscall.SysProcIDMap{{ContainerID: uid, HostID: uid, Size: 1}},
		GidMappings:                []syscall.SysProcIDMap{{ContainerID: gid, HostID: gid, Size: 1}},
		GidMappingsEnableSetgroups: false,
		AmbientCaps:                []uintptr{unix.CAP_NET_ADMIN},
		Pdeathsig:                  syscall.SIGKILL,
	}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start netchild: %w", err)
	}
	childEnd.Close()

	tapFD, err := recvFD(hubEnd)
	if err != nil {
		return fmt.Errorf("receive tap fd: %w", err)
	}
	fmt.Fprintf(os.Stderr, "netns: tap ready after %v\n", time.Since(t0))

	nat, err := newNAT(tapFD, natConfig{Allow: splitCSV(allow), GatewayLoopback: gwLoop, GatewayPorts: gwPorts})
	if err != nil {
		cmd.Process.Kill()
		return fmt.Errorf("netstack: %w", err)
	}
	if forward != "" {
		laddr, port, ok := strings.Cut(forward, "=")
		if !ok {
			return fmt.Errorf("bad -forward %q", forward)
		}
		if err := nat.forward(laddr, port); err != nil {
			cmd.Process.Kill()
			return err
		}
	}
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	go func() { <-sig; cmd.Process.Kill() }()
	fmt.Fprintf(os.Stderr, "netns: nat up after %v\n", time.Since(t0))
	err = cmd.Wait()
	fmt.Fprintf(os.Stderr, "netns: exited after %v\n", time.Since(t0))
	nat.stats(os.Stderr)
	return err
}

func splitCSV(s string) []string {
	if s == "" {
		return nil
	}
	return strings.Split(s, ",")
}

func splitPorts(s string) []uint16 {
	var out []uint16
	for _, p := range splitCSV(s) {
		var n uint16
		fmt.Sscan(p, &n)
		out = append(out, n)
	}
	return out
}
