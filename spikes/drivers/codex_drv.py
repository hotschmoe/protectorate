import sys, json, lib
cwd, log, prompt, thread = sys.argv[1:5]
policy = sys.argv[5] if len(sys.argv) > 5 else "untrusted"
import os
p = lib.Proc(os.environ.get("CODEX_CMD", "codex app-server").split(), cwd, log)
nid = [0]
def call(method, params):
    nid[0] += 1; i = nid[0]
    p.send({"jsonrpc": "2.0", "id": i, "method": method, "params": params})
    return i
def wait(i):
    while True:
        m = p.recv(120)
        if m is None: raise SystemExit("eof")
        if m.get("id") == i and "method" not in m: return m
        handle(m)
def handle(m):
    meth = m.get("method")
    if meth and "id" in m:
        print("SERVER REQUEST:", json.dumps(m)[:700])
        p.send({"jsonrpc": "2.0", "id": m["id"], "result": {"decision": "accept"}})
    else:
        if "delta" not in meth: print(meth, json.dumps(m.get("params"))[:160])
        return meth
i = call("initialize", {"clientInfo": {"name": "prot-drivers", "version": "0"}, "capabilities": {"experimentalApi": True}})
print(json.dumps(wait(i))[:300])
p.send({"jsonrpc": "2.0", "method": "initialized"})
if thread == "new":
    r = wait(call("thread/start", {"cwd": cwd, "approvalPolicy": policy, "sandbox": "workspace-write"}))
else:
    r = wait(call("thread/resume", {"threadId": thread, "cwd": cwd, "approvalPolicy": policy, "sandbox": "workspace-write"}))
print("THREAD:", json.dumps(r)[:600])
tid = r["result"]["thread"]["id"]
call("turn/start", {"threadId": tid, "input": [{"type": "text", "text": prompt}]})
while True:
    m = p.recv(180)
    if m is None: break
    meth = handle(m)
    if meth == "turn/completed": break
p.p.stdin.close()
try: p.p.wait(10)
except Exception: p.p.kill()
