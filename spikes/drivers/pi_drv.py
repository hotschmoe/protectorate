import sys, json, os, lib
cwd, log, prompt = sys.argv[1:4]
extra = sys.argv[4:]
p = lib.Proc(["pi", "--mode", "rpc"] + extra, cwd, log)
p.send({"id": "s1", "type": "get_state"})
p.send({"id": "p1", "type": "prompt", "message": prompt})
while True:
    m = p.recv(240)
    if m is None: break
    t = m.get("type")
    if t == "extension_ui_request":
        print("UI REQUEST:", json.dumps(m)[:600])
        p.send({"type": "extension_ui_response", "id": m["id"], "confirmed": True})
        continue
    if t == "message_update": continue
    print(t, json.dumps(m)[:300])
    if t == "agent_settled": break
p.send({"id": "s2", "type": "get_state"})
m = p.recv(10); print(json.dumps(m)[:800])
p.p.stdin.close()
try: p.p.wait(10)
except Exception: p.p.kill()
