import sys, json, os, lib
cwd, log, prompt, sid = sys.argv[1:5]
cmd = os.environ.get("ACP_CMD", "grok agent stdio").split()
p = lib.Proc(cmd, cwd, log)
nid = [0]
def call(method, params):
    nid[0] += 1
    p.send({"jsonrpc": "2.0", "id": nid[0], "method": method, "params": params})
    return nid[0]
def handle(m):
    meth = m.get("method")
    if meth and "id" in m:
        print("SERVER REQUEST:", json.dumps(m)[:900])
        opts = m["params"].get("options", [])
        pick = next((o["optionId"] for o in opts if o.get("kind", "").startswith("allow")), opts[0]["optionId"] if opts else None)
        p.send({"jsonrpc": "2.0", "id": m["id"], "result": {"outcome": {"outcome": "selected", "optionId": pick}}})
    elif meth:
        upd = m.get("params", {}).get("update", {})
        k = upd.get("sessionUpdate", meth)
        if "chunk" not in k and not k.startswith("_x.ai") and "commands" not in k: print("NOTIF", k, json.dumps(m.get("params"))[:220])
def wait(i):
    while True:
        m = p.recv(120)
        if m is None: raise SystemExit("eof")
        if m.get("id") == i and "method" not in m: return m
        handle(m)
r = wait(call("initialize", {"protocolVersion": 1, "clientCapabilities": {}, "clientInfo": {"name": "prot-drivers", "version": "0"}}))
print("INIT:", json.dumps(r)[:700])
if sid == "new":
    r = wait(call("session/new", {"cwd": cwd, "mcpServers": []}))
    print("NEW:", json.dumps(r)[:500]); sid = r["result"]["sessionId"]
else:
    r = wait(call("session/load", {"sessionId": sid, "cwd": cwd, "mcpServers": []}))
    print("LOAD:", json.dumps(r)[:500])
if os.environ.get("PRE"):
    print("PRE RESULT:", json.dumps(wait(call("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": os.environ["PRE"]}]})))[:400])
i = call("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": prompt}]})
r = wait(i)
print("PROMPT RESULT:", json.dumps(r)[:500])
p.p.stdin.close()
try: p.p.wait(10)
except Exception: p.p.kill()
