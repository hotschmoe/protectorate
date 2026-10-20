package sleeve

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// DelegatedCgroupRoot exists only inside a nesting-enabled sandbox.
const DelegatedCgroupRoot = "/run/protectorate/cgroup"

// Only delegated/ is exposed. The scope's resource controls stay outside
// both the mount namespace and the cgroup namespace of the sleeve.
const delegateScope = `
root=$1; setup=$2; shift 2
printf '256\n' > "$root/cgroup.max.descendants"
printf '16\n' > "$root/cgroup.max.depth"
/usr/bin/mkdir "$root/delegated"
printf '%s\n' "$$" > "$root/delegated/cgroup.procs"
printf '+cpu +memory +pids\n' > "$root/cgroup.subtree_control"
exec /usr/bin/unshare --user --map-current-user --cgroup /bin/sh -ec "$setup" sh "$root/delegated" "$@"
`

const delegateRun = `
root=$1; shift
/usr/bin/mkdir "$root/run"
printf '%s\n' "$$" > "$root/run/cgroup.procs"
printf '+cpu +memory +pids\n' > "$root/cgroup.subtree_control"
exec "$@"
`

// The supervisor stays in the parent's run/ leaf. No child command runs
// until its cgroup limits have been installed and it has moved into it.
// The lock also serializes cleanup with respawns of the same unit.
const runChildScope = `
root=$1; cpu=$2; memory=$3; tasks=$4; lock=$5; shift 5
/usr/bin/mkdir -p "${lock%/*}"
exec 5>"$lock"
/usr/bin/flock --exclusive 5
if test -d "$root"; then
  /usr/bin/find "$root" -depth -type d -exec /usr/bin/rmdir '{}' \;
fi
/usr/bin/mkdir "$root"
finish() {
  status=$?
  trap - EXIT
  printf '1\n' 2>/dev/null > "$root/cgroup.kill" || :
  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    /usr/bin/find "$root" -depth -type d -exec /usr/bin/rmdir '{}' \; 2>/dev/null || :
    test -d "$root" || break
    /usr/bin/sleep .01
  done
  exit "$status"
}
trap finish EXIT
trap 'exit 143' TERM HUP
trap ':' INT QUIT
printf '%s\n' "$cpu" > "$root/cpu.max"
printf '%s\n' "$memory" > "$root/memory.max"
test ! -e "$root/memory.swap.max" || printf '0\n' > "$root/memory.swap.max"
printf '%s\n' "$tasks" > "$root/pids.max"
exec 4<&0
/usr/bin/env --default-signal=INT,QUIT /bin/sh -ec 'root=$1; shift; /usr/bin/mkdir "$root/run"; printf "%s\n" "$$" > "$root/run/cgroup.procs"; exec "$@"' sh "$root" "$@" <&4 &
child=$!
exec 4<&-
set +e
while :; do
  wait "$child"
  status=$?
  kill -0 "$child" 2>/dev/null || break
done
exit "$status"
`

func (c Config) scopeArgv(b Box, argv []string) []string {
	root := filepath.Join(c.CgroupRoot, b.Unit+".scope")
	if b.Nested {
		argv = append([]string{"/bin/sh", "-ec", delegateScope, "sh", root, delegateRun}, argv...)
	}
	if !c.delegated {
		return argv
	}
	cpu, memory := "max 100000", "max"
	if b.Limits.CPUPercent > 0 {
		cpu = strconv.FormatUint(uint64(b.Limits.CPUPercent)*1000, 10) + " 100000"
	}
	if b.Limits.MemoryBytes > 0 {
		memory = strconv.FormatUint(b.Limits.MemoryBytes, 10)
	}
	lock := filepath.Join(c.Home, "runtime", b.Unit+".lock")
	return append([]string{"/bin/sh", "-ec", runChildScope, "sh", root, cpu, memory, strconv.Itoa(c.TasksMax), lock}, argv...)
}

func (c Config) killChildScope(ctx context.Context, name string) error {
	root := c.scopeDir(name)
	if err := os.WriteFile(filepath.Join(root, "cgroup.kill"), []byte("1"), 0); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("kill child scope %s: %w", name, err)
	}
	deadline := time.Now().Add(5 * time.Second)
	for {
		err := removeCgroups(ctx, root)
		if err == nil || os.IsNotExist(err) {
			return nil
		}
		if ctx.Err() != nil {
			return fmt.Errorf("remove child scope %s: %w", name, ctx.Err())
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("remove child scope %s: %w", name, err)
		}
		select {
		case <-ctx.Done():
		case <-time.After(scopePollInterval):
		}
	}
}

func removeCgroups(ctx context.Context, root string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	entries, err := os.ReadDir(root)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if entry.IsDir() {
			if err := removeCgroups(ctx, filepath.Join(root, entry.Name())); err != nil && !os.IsNotExist(err) {
				return err
			}
		}
	}
	return os.Remove(root)
}

func cgroupPopulated(root string) bool {
	b, err := os.ReadFile(filepath.Join(root, "cgroup.events"))
	return err == nil && strings.Contains(string(b), "populated 1\n")
}
