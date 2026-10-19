package main

import (
	"fmt"
	"os"
	"syscall"
	"unsafe"

	"golang.org/x/sys/unix"
)

// netChild runs inside the new user+net namespace with CAP_NET_ADMIN ambient.
// fd 3 is the unix socket to the hub.
func netChild(argv []string) {
	if err := setupTap(argv); err != nil {
		fmt.Fprintln(os.Stderr, "netchild:", err)
		os.Exit(1)
	}
}

func setupTap(argv []string) error {
	tun, err := unix.Open("/dev/net/tun", unix.O_RDWR|unix.O_CLOEXEC, 0)
	if err != nil {
		return fmt.Errorf("open /dev/net/tun: %w", err)
	}
	var ifr [40]byte
	copy(ifr[:], "tap0")
	*(*uint16)(unsafe.Pointer(&ifr[16])) = unix.IFF_TAP | unix.IFF_NO_PI
	if _, _, e := unix.Syscall(unix.SYS_IOCTL, uintptr(tun), unix.TUNSETIFF, uintptr(unsafe.Pointer(&ifr[0]))); e != 0 {
		return fmt.Errorf("TUNSETIFF: %w", e)
	}
	s, err := unix.Socket(unix.AF_INET, unix.SOCK_DGRAM|unix.SOCK_CLOEXEC, 0)
	if err != nil {
		return err
	}
	defer unix.Close(s)
	if err := ifAddr(s, "tap0", sleeveIP, "255.255.255.0"); err != nil {
		return err
	}
	if err := ifUp(s, "tap0"); err != nil {
		return err
	}
	if err := ifUp(s, "lo"); err != nil {
		return err
	}
	if err := defRoute(s, gwIP, "tap0"); err != nil {
		return err
	}
	if err := sendFD(3, tun); err != nil {
		return fmt.Errorf("send fd: %w", err)
	}
	unix.Close(tun)
	// Hand the namespace to the real command; clear the ambient cap so it is not inherited.
	if err := unix.Prctl(unix.PR_CAP_AMBIENT, unix.PR_CAP_AMBIENT_CLEAR_ALL, 0, 0, 0); err != nil {
		return fmt.Errorf("clear ambient: %w", err)
	}
	cmd := argv[0]
	if len(cmd) == 0 {
		return fmt.Errorf("no command")
	}
	path, err := lookPath(cmd)
	if err != nil {
		return err
	}
	return syscall.Exec(path, argv, os.Environ())
}

func lookPath(c string) (string, error) {
	for _, d := range append([]string{""}, "/usr/bin/", "/bin/") {
		p := d + c
		if d == "" && c[0] != '/' {
			continue
		}
		if _, err := os.Stat(p); err == nil {
			return p, nil
		}
	}
	return "", fmt.Errorf("not found: %s", c)
}

func sockaddrIn(ip string) [16]byte {
	var a [16]byte
	*(*uint16)(unsafe.Pointer(&a[0])) = unix.AF_INET
	var b [4]byte
	fmt.Sscanf(ip, "%d.%d.%d.%d", &b[0], &b[1], &b[2], &b[3])
	copy(a[4:8], b[:])
	return a
}

func ifreq(name string) [40]byte {
	var r [40]byte
	copy(r[:], name)
	return r
}

func ioctl(fd int, req uintptr, p unsafe.Pointer) error {
	if _, _, e := unix.Syscall(unix.SYS_IOCTL, uintptr(fd), req, uintptr(p)); e != 0 {
		return e
	}
	return nil
}

func ifAddr(s int, name, ip, mask string) error {
	r := ifreq(name)
	a := sockaddrIn(ip)
	copy(r[16:], a[:])
	if err := ioctl(s, unix.SIOCSIFADDR, unsafe.Pointer(&r[0])); err != nil {
		return fmt.Errorf("SIOCSIFADDR: %w", err)
	}
	m := sockaddrIn(mask)
	copy(r[16:], m[:])
	if err := ioctl(s, unix.SIOCSIFNETMASK, unsafe.Pointer(&r[0])); err != nil {
		return fmt.Errorf("SIOCSIFNETMASK: %w", err)
	}
	return nil
}

func ifUp(s int, name string) error {
	r := ifreq(name)
	if err := ioctl(s, unix.SIOCGIFFLAGS, unsafe.Pointer(&r[0])); err != nil {
		return fmt.Errorf("SIOCGIFFLAGS %s: %w", name, err)
	}
	*(*uint16)(unsafe.Pointer(&r[16])) |= unix.IFF_UP | unix.IFF_RUNNING
	if err := ioctl(s, unix.SIOCSIFFLAGS, unsafe.Pointer(&r[0])); err != nil {
		return fmt.Errorf("SIOCSIFFLAGS %s: %w", name, err)
	}
	return nil
}

func defRoute(s int, gw, dev string) error {
	var rt [120]byte
	z := sockaddrIn("0.0.0.0")
	g := sockaddrIn(gw)
	copy(rt[8:], z[:])
	copy(rt[24:], g[:])
	copy(rt[40:], z[:])
	*(*uint16)(unsafe.Pointer(&rt[56])) = unix.RTF_UP | unix.RTF_GATEWAY
	if err := ioctl(s, unix.SIOCADDRT, unsafe.Pointer(&rt[0])); err != nil {
		return fmt.Errorf("SIOCADDRT: %w", err)
	}
	return nil
}

func sendFD(sock, fd int) error {
	return unix.Sendmsg(sock, []byte{0}, unix.UnixRights(fd), nil, 0)
}

func recvFD(f *os.File) (int, error) {
	buf := make([]byte, 1)
	oob := make([]byte, unix.CmsgSpace(4))
	_, oobn, _, _, err := unix.Recvmsg(int(f.Fd()), buf, oob, 0)
	if err != nil {
		return -1, err
	}
	msgs, err := unix.ParseSocketControlMessage(oob[:oobn])
	if err != nil || len(msgs) == 0 {
		return -1, fmt.Errorf("no control message: %v", err)
	}
	fds, err := unix.ParseUnixRights(&msgs[0])
	if err != nil || len(fds) == 0 {
		return -1, fmt.Errorf("no fd: %v", err)
	}
	return fds[0], nil
}
