// Package jobtest is a portable stand-in for sh in job tests: a test binary
// re-executes itself as a tiny scripted process, so the same tests run on
// Linux, macOS and Windows.
//
// A package using it calls Run first in TestMain and builds commands with
// Script. Steps run in order:
//
//	say:TEXT       print TEXT and a newline
//	err:TEXT       the same on stderr
//	env:A,B        print the values of A and B, space separated
//	exit:N         exit with status N
//	sleep:SECONDS  sleep, fractions allowed
//	ignoreterm     ignore the polite termination signals
//	spawn:SECONDS  start a child that sleeps, print "child=PID", do not wait
//	wait           wait for spawned children
//	flood:N        write N bytes of x
//	cat:FILE       copy FILE to stdout
//	append:F:TEXT  append TEXT and a newline to F
//	appendpid:F    append this process id and a newline to F
//	lines:F        print the number of lines in F
//	pwd            print the working directory
//	write:F:TEXT   write TEXT and a newline to F, creating its directory
//	floodfile:F:N  write N bytes of x to F, creating its directory
//	chmod:F:OCTAL  set the permission bits of F
package jobtest

import (
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const marker = "--jobtest-script"

// Run executes the script when this process was started by Script and exits;
// otherwise it returns at once.
func Run() {
	if len(os.Args) > 1 && os.Args[1] == marker {
		os.Exit(run(os.Args[2:]))
	}
}

// Args returns the arguments that make a copy of the binary run steps; use it
// when the command must be a file other than the running test binary.
func Args(steps ...string) []string { return append([]string{marker}, steps...) }

// Script returns the argv that runs steps in a re-executed copy of the
// current binary.
func Script(steps ...string) []string {
	exe, err := os.Executable()
	if err != nil {
		panic(err)
	}
	return append([]string{exe, marker}, steps...)
}

// Alive reports whether a process with this id is running.
func Alive(pid int) bool {
	if raw, err := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/stat"); err == nil {
		s := string(raw)
		return s[strings.LastIndex(s, ")")+2] != 'Z'
	}
	if runtime.GOOS == "windows" {
		out, _ := exec.Command("tasklist", "/FI", "PID eq "+strconv.Itoa(pid), "/NH").Output()
		return strings.Contains(string(out), " "+strconv.Itoa(pid)+" ")
	}
	p, err := os.FindProcess(pid)
	return err == nil && p.Signal(syscall.Signal(0)) == nil
}

func run(steps []string) int {
	var children []*exec.Cmd
	for _, s := range steps {
		op, arg, _ := strings.Cut(s, ":")
		switch op {
		case "say":
			fmt.Fprintln(os.Stdout, arg)
		case "err":
			fmt.Fprintln(os.Stderr, arg)
		case "env":
			var vals []string
			for _, k := range strings.Split(arg, ",") {
				vals = append(vals, os.Getenv(k))
			}
			fmt.Fprintln(os.Stdout, strings.Join(vals, " "))
		case "exit":
			n, _ := strconv.Atoi(arg)
			return n
		case "sleep":
			f, _ := strconv.ParseFloat(arg, 64)
			time.Sleep(time.Duration(f * float64(time.Second)))
		case "ignoreterm":
			signal.Ignore(os.Interrupt, syscall.SIGTERM)
		case "spawn":
			c := exec.Command(os.Args[0], marker, "sleep:"+arg)
			if err := c.Start(); err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
			children = append(children, c)
			fmt.Fprintf(os.Stdout, "child=%d\n", c.Process.Pid)
		case "wait":
			for _, c := range children {
				_ = c.Wait()
			}
		case "flood":
			n, _ := strconv.Atoi(arg)
			buf := []byte(strings.Repeat("x", 1<<16))
			for n > 0 {
				k := min(n, len(buf))
				os.Stdout.Write(buf[:k])
				n -= k
			}
		case "cat":
			b, err := os.ReadFile(arg)
			if err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
			os.Stdout.Write(b)
		case "append", "appendpid":
			file, text, _ := strings.Cut(arg, ":")
			if op == "appendpid" {
				file, text = arg, strconv.Itoa(os.Getpid())
			}
			f, err := os.OpenFile(file, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
			if err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
			fmt.Fprintln(f, text)
			f.Close()
		case "lines":
			b, err := os.ReadFile(arg)
			if err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
			fmt.Fprintln(os.Stdout, strings.Count(string(b), "\n"))
		case "pwd":
			wd, _ := os.Getwd()
			fmt.Fprintln(os.Stdout, wd)
		case "write", "floodfile":
			file, rest, _ := strings.Cut(arg, ":")
			data := []byte(rest + "\n")
			if op == "floodfile" {
				n, _ := strconv.Atoi(rest)
				data = []byte(strings.Repeat("x", n))
			}
			if err := os.MkdirAll(filepath.Dir(file), 0o755); err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
			if err := os.WriteFile(file, data, 0o644); err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
		case "chmod":
			file, mode, _ := strings.Cut(arg, ":")
			m, _ := strconv.ParseUint(mode, 8, 32)
			if err := os.Chmod(file, os.FileMode(m)); err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
		default:
			fmt.Fprintf(os.Stderr, "unknown step %q\n", s)
			return 2
		}
	}
	return 0
}

// QuietGit turns off git's background auto-maintenance for every git process
// the tests start, directly or through the code under test. Newer git runs it
// detached after fetch, and it can still be writing into a repository when
// the test's TempDir cleanup runs.
func QuietGit() {
	kv := [][2]string{{"gc.auto", "0"}, {"maintenance.auto", "false"}}
	os.Setenv("GIT_CONFIG_COUNT", strconv.Itoa(len(kv)))
	for i, p := range kv {
		os.Setenv(fmt.Sprintf("GIT_CONFIG_KEY_%d", i), p[0])
		os.Setenv(fmt.Sprintf("GIT_CONFIG_VALUE_%d", i), p[1])
	}
}
