import socket, os, base64, struct, json, sys, select, time

class WS:
    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX); self.s.connect(path)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(("GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                        "Sec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n\r\n" % key).encode())
        buf = b""
        while b"\r\n\r\n" not in buf: buf += self.s.recv(4096)
        head, self.buf = buf.split(b"\r\n\r\n", 1)
        print("HANDSHAKE:", head.decode().splitlines()[0])
    def send(self, obj):
        d = json.dumps(obj).encode(); mask = os.urandom(4)
        n = len(d); hdr = bytes([0x81])
        hdr += bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n) if n < 65536 else bytes([0x80 | 127]) + struct.pack(">Q", n)
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(d)))
    def _need(self, n):
        while len(self.buf) < n: self.buf += self.s.recv(65536)
        r, self.buf = self.buf[:n], self.buf[n:]; return r
    def recv(self):
        while True:
            b0, b1 = self._need(2); n = b1 & 0x7f
            if n == 126: n = struct.unpack(">H", self._need(2))[0]
            elif n == 127: n = struct.unpack(">Q", self._need(8))[0]
            d = self._need(n); op = b0 & 0xf
            if op == 1: return json.loads(d)
            if op == 8: return None
            if op == 9: self.s.sendall(b"\x8a\x00")

if __name__ == "__main__":
    cwd, prompt, thread = sys.argv[1:4]
    sock = os.path.realpath(os.path.expanduser("~/.codex/app-server-control/app-server-control.sock"))
    w = WS(sock); nid = [0]
    def call(m, p):
        nid[0] += 1; w.send({"jsonrpc": "2.0", "id": nid[0], "method": m, "params": p}); return nid[0]
    def wait(i):
        while True:
            m = w.recv()
            if m.get("id") == i and "method" not in m: return m
            if "method" in m and "id" in m:
                print("SERVER REQUEST", json.dumps(m)[:400]); w.send({"jsonrpc": "2.0", "id": m["id"], "result": {"decision": "accept"}})
    print(json.dumps(wait(call("initialize", {"clientInfo": {"name": "prot-drivers", "version": "0"}}))) [:300])
    w.send({"jsonrpc": "2.0", "method": "initialized"})
    r = wait(call("thread/resume", {"threadId": thread, "cwd": cwd, "excludeTurns": True}))
    print("RESUME:", json.dumps(r)[:300])
    if "error" in r: sys.exit(1)
    call("turn/start", {"threadId": thread, "input": [{"type": "text", "text": prompt}]})
    while True:
        m = w.recv()
        meth = m.get("method", "")
        if meth == "item/completed" and m["params"]["item"]["type"] == "agentMessage": print("AGENT:", m["params"]["item"]["text"])
        if meth == "turn/completed": print("TURN COMPLETED"); break
