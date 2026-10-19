"""Replays each CLI's observed credential write pattern on FAKE files.

Usage (inside a container):
  replay.py write <cli> <path-as-the-cli-sees-it> <new-access-token>
  replay.py read  <path>
  replay.py hold-lock <cli> <path> <seconds>      # holds the CLI's lock
  replay.py try-lock  <cli> <path>                # prints LOCKED or FREE
Patterns (from strace / source, see README):
  claude: mkdir <dir>/.oauth_refresh.lock ; write .credentials.json.tmp.X (O_EXCL,0600) ; rename ; rmdir
  codex : open(auth.json, O_WRONLY|O_CREAT|O_TRUNC) in place, no lock
  grok  : flock(LOCK_EX) GROK_HOME/auth.json.lock ; resolve symlink ; tmp next to target ; rename
  pi    : mkdir auth.json.lock dir ; open(auth.json, O_TRUNC) in place ; rmdir
"""
import errno, fcntl, json, os, sys, time


def doc(token):
    return json.dumps({"claudeAiOauth": {"accessToken": token, "refreshToken": "FAKE-RT-" + token}})


def atomic_write(path, data):
    d = os.path.dirname(path)
    tmp = os.path.join(d, os.path.basename(path) + ".tmp.%d" % os.getpid())
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.write(fd, data.encode())
    os.close(fd)
    os.rename(tmp, path)


def lock_path(cli, p):
    d = os.path.dirname(p)
    return {"claude": d + "/.oauth_refresh.lock", "grok": d + "/auth.json.lock",
            "pi": p + ".lock", "codex": None}[cli]


def write(cli, p, token):
    if cli == "claude":
        os.mkdir(lock_path(cli, p))
        try:
            atomic_write(p, doc(token))
        finally:
            os.rmdir(lock_path(cli, p))
    elif cli == "codex":
        fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        os.write(fd, doc(token).encode())
        os.close(fd)
    elif cli == "grok":
        lf = os.open(lock_path(cli, p), os.O_RDWR | os.O_CREAT, 0o666)
        fcntl.flock(lf, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            atomic_write(os.path.realpath(p), doc(token))  # grok resolves the symlink first
        finally:
            os.close(lf)
    elif cli == "pi":
        os.mkdir(lock_path(cli, p))
        try:
            fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            os.write(fd, doc(token).encode())
            os.close(fd)
        finally:
            os.rmdir(lock_path(cli, p))


def hold(cli, p, secs):
    lp = lock_path(cli, p)
    if cli in ("claude", "pi"):
        os.mkdir(lp)
        print("HELD", flush=True)
        time.sleep(secs)
        os.rmdir(lp)
    else:
        lf = os.open(lp, os.O_RDWR | os.O_CREAT, 0o666)
        fcntl.flock(lf, fcntl.LOCK_EX | fcntl.LOCK_NB)
        print("HELD", flush=True)
        time.sleep(secs)


def try_lock(cli, p):
    lp = lock_path(cli, p)
    try:
        if cli in ("claude", "pi"):
            os.mkdir(lp)
            os.rmdir(lp)
        else:
            lf = os.open(lp, os.O_RDWR | os.O_CREAT, 0o666)
            fcntl.flock(lf, fcntl.LOCK_EX | fcntl.LOCK_NB)
        print("FREE")
    except (FileExistsError, BlockingIOError):
        print("LOCKED")


def read(p):
    print(json.load(open(p))["claudeAiOauth"]["accessToken"])


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[0] == "write":
        write(a[1], a[2], a[3])
    elif a[0] == "read":
        read(a[1])
    elif a[0] == "hold-lock":
        hold(a[1], a[2], float(a[3]))
    elif a[0] == "try-lock":
        try_lock(a[1], a[2])
    elif a[0] == "rename-over":
        try:
            atomic_write(a[1], doc("X"))
            print("rename ok")
        except OSError as e:
            print("rename failed:", errno.errorcode[e.errno])
