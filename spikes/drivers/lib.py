import json, subprocess, sys, time, threading, queue

class Proc:
    def __init__(self, cmd, cwd, log, env=None):
        self.t0 = time.time()
        self.log = open(log, "a")
        self.p = subprocess.Popen(cmd, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, bufsize=1, env=env)
        self.q = queue.Queue()
        threading.Thread(target=self._rd, args=(self.p.stdout, "out"), daemon=True).start()
        threading.Thread(target=self._rd, args=(self.p.stderr, "err"), daemon=True).start()

    def _rd(self, f, kind):
        for line in f:
            line = line.rstrip("\n")
            self.log.write("%.2f %s %s\n" % (time.time() - self.t0, kind, line)); self.log.flush()
            if kind == "out":
                try: self.q.put(json.loads(line))
                except Exception: self.q.put({"_raw": line})
        self.q.put(None)

    def send(self, obj):
        s = json.dumps(obj)
        self.log.write("%.2f in  %s\n" % (time.time() - self.t0, s)); self.log.flush()
        self.p.stdin.write(s + "\n"); self.p.stdin.flush()

    def recv(self, timeout=120):
        return self.q.get(timeout=timeout)
