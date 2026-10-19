import sys, json, lib
cwd, log, mode, prompt = sys.argv[1:5]
extra = sys.argv[5:]
cmd = ["claude", "-p", "--output-format", "stream-json", "--input-format", "stream-json", "--verbose",
       "--permission-prompt-tool", "stdio", "--permission-mode", mode] + extra
p = lib.Proc(cmd, cwd, log)
p.send({"type": "user", "message": {"role": "user", "content": prompt}})
while True:
    m = p.recv(180)
    if m is None: break
    t = m.get("type")
    if t == "control_request":
        print("APPROVAL REQUEST:", json.dumps(m)[:600])
        req = m["request"]
        p.send({"type": "control_response", "response": {"subtype": "success", "request_id": m["request_id"],
                "response": {"behavior": "allow", "updatedInput": req.get("input", {})}}})
    elif t == "result":
        print("RESULT:", json.dumps(m)[:500]); break
    else:
        print(t, m.get("subtype", ""), json.dumps(m)[:200])
p.p.stdin.close()
try: p.p.wait(15)
except Exception: p.p.kill()
