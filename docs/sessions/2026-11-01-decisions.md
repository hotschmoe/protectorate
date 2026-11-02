# 2026-11-01: open decisions and discussion points

Each item has Claude's recommendation. Reply per item with agree, disagree or a comment. The answers are recorded in SPEC.md section 13. Where this file and SPEC.md disagree, SPEC.md wins.

## A. Discussion points

A1. **Keep dev-hotschmoe in production until `next` reaches parity.** `next` is a rewrite. ms-r1 stays on v0.6.x until `next` can run sleeves, attach and resleeve.
Rec: yes.

A2. **The network is the least proven part.** Stage 0 is a throwaway spike that proves four things:
- headscale's control and DERP paths work through Traefik;
- the tailnet client (`member`) joins and answers on Linux, Windows and Android;
- the one-rule policy blocks a flow started by a node;
- scan to join works.

If any of these fail, the design changes before core code exists.
Rec: yes, do it first (D31).

A3. **The two biggest pieces are the hub-owned terminals (PTY, terminal emulation, attach) and the Flutter UI.** Both are built right after the core, with one harness (Claude headless), so they get used early.
Rec: as in the build order.

A4. **Two things are unverified, and each is checked before we build on it:**
- presence detection on Windows from a LocalSystem service;
- whether GrokBot, Claude Duet and ChatGPT Dot can call a local CLI or a remote MCP at all.

Rec: check both during the stage 0 spike.

A5. **PROBLEMS.md coverage:**
- Covered: P1 quota, P3 cross-arch, P4 phone, P5 harnesses, P7, P8 clobbering, P10 (`pi_vendor`).
- P2 is covered by logins held on the hub, plus maintenance jobs that are allowed when a machine is idle or overridden per job.
- P6 backlog is D29.
- P9 leases are D30, deferred.
- P11 Windows GUI programs stay shelved; use a program's own API through node jobs.

Rec: accept this mapping.

A6. **Verus has real costs: the pinned toolchain, and proofs that must keep up with code changes.** Phasing (D28) measures that cost on two small modules before anything else is verified. Verified modules should be small, with interfaces that rarely change.
Rec: yes.

A7. **Updates (SPEC 6a): hub and nodes pull releases from GitHub, verified by a signature.**
- The hub snapshots headscale's database, re-execs in place and rolls back automatically.
- Nodes update when the hub tells them to, and roll themselves back if the hub has not contacted them within 10 minutes.
- Rollout order: hub, then a canary per OS and architecture, then the rest.
- New cost: a signing step in release CI, with its own key to look after.

Rec: yes. The signing key lives in a GitHub Actions secret plus an offline backup.

A8. **Moving the hub (SPEC 6b) is a backup restore.**
- `hub export` quiesces sleeves, writes the backup and leaves a tombstone on the old hub.
- `hub import` restores on the new machine.
- Repoint Traefik and the firewall rule. Nodes don't re-enroll.
- It is the same path as recovering a dead hub, so one test covers both.

Rec: yes.

## B. Decisions likely to pass as a block

B1. **D3:** split-horizon DNS for protectorate.hotschmoe.com at both sites and in the phone's WireGuard DNS. Rec: yes; home-site nodes need it to find the hub's headscale.

B2. **D5:** join by scanning a QR code in the app; pasting the code also works. Rec: yes.

B3. **D6:** agent tokens for vendor general agents, each with a scope ceiling. Rec: yes.

B4. **D7:** child sleeves survive a root sleeve crash, but die when the root is stopped or removed. Rec: yes. A crash triggers a resleeve of the root, and the children wait for it.

B5. **D8:** a child sleeve's workspace is a git worktree by default. Rec: yes.

B6. **D11:** the Windows node runs as a LocalSystem service, and jobs run under a low-privilege runner account. Rec: yes.

B7. **D13:** Antigravity (`agy`) is pane-only until its approvals work headless. Rec: yes.

B8. **D15:** each sleeve has an ordered resleeve target list, defaulting to `[pi_local]`. Rec: yes.

B9. **D16:** guardrail thresholds as listed in SPEC, with no automatic revert of edits to forbidden paths. Code stops the sleeve and you decide. Rec: yes.

B10. **D17:** the hub updates by re-exec in place, keeping sleeves alive. Rec: yes.

B11. **D18:** the hub never types into a pane after launch. Rec: yes. It is hands-off toward harnesses.

B12. **D22:** vector search stays off until keyword (FTS5) search falls short. Rec: yes.

B13. **D23:** the brief file is `.protectorate/BRIEF.md`, built by code. An LLM-written summary is added only for planned resleeves. Rec: yes.

B14. **D24:** the app has a `===>` command line and an F-key legend. Rec: yes.

B15. **D25:** macOS node and desktop app only once a Mac exists. Rec: yes.

B16. **D27:** code derives the task-size class for dispatch from the median of the last N tasks; Envoy may nudge it by one class, and code clamps the result. Rec: yes.

## C. Decisions that need your call

C1. **D1:** headscale ships inside the hub binary but runs as a separate sandboxed child process, not in-process. Rec: yes. In-process, headscale takes over signal handling and can exit the whole process on an error (log.Fatal).

C2. **D2:** build headscale from pinned upstream source in our CI, or embed the official release binary. Rec: build from source. It is reproducible, CGO-free, and shares one go.mod with `member`, so the client and server tailscale versions match.

C3. **D9:** how vendor general agents reach Protectorate. Rec: a remote MCP served by the hub, plus `protectorate-node client` as a CLI for apps that run commands locally. Check what each app actually supports before building (A4).

C4. **D10:** the Linux node runs as a system service, which costs one sudo at install. Rec: yes. It survives reboots and logouts, and presence detection needs logind access.

C5. **D12:** drop the sandbox's network namespace and egress proxy. Rec: yes. The sandbox stops sleeves stepping on each other; it is not a security boundary against a hostile agent. Revisit before hostile code is ever in scope.

C6. An experiment was removed on 2026-11-02; its design and this decision remain in git history.

C7. **D19:** an exemption from Verus rule 6, so the sandbox and presence crates may use `unsafe` for FFI without Verus proofs. Rec: yes. These are OS calls Verus cannot reason about; each sits in a small private helper.

C8. **D20:** the GitHub push token lives on the hub only, and sleeves push through `protectorate git push`. Rec: yes. A prompt-injected sleeve then cannot steal it. Sleeves get a read-only token for `gh` queries.

C9. **D21:** index your own interactive CLI sessions too, or only sessions the hub launched. Rec: hub-launched only, so your private sessions stay out of the index.

C10. **D26:** who can read history. Rec: Envoy, plus a resleeved sleeve for its own earlier run. Anything wider is you only.

C11. **D28:** Verus phases.
- Phase 1: the journal codec and the node frame codec.
- Phase 2: PTY ring, line framer and log tail.
- Phase 3: only if phases 1 and 2 stay cheap to maintain.
- Presence, audit and brief math are dropped from scope.

Rec: yes.

C12. **D29:** a backlog that code works through when quota is spare, plus a review inbox for public repos (PROBLEMS P6). Rec: yes, after stage 3 (resleeve).

C13. **D30:** leases on GPUs and other resources, with a TTL (PROBLEMS P9). Rec: later. Build them as a hub-side table when two jobs actually collide on a GPU.

C14. **D31:** a throwaway network spike as stage 0. Rec: yes (A2).

## D. New from your notes on 2026-11-01

D1n. **D32: one kind of node.** Tiers are gone and presence gates jobs. Details:
- UNKNOWN counts as present, so a node whose detector fails takes no jobs, and `doctor` reports it.
- ssh and other remote logins don't count as presence; only local seat input and lock state do.
- The node itself refuses to start a job while someone is present, so a stale view on the hub can't put work on a busy machine.
- You can override one job with `--even-if-present`; that is how urgent maintenance reaches a machine in use.
- When a person arrives mid-job, the job drops to idle CPU and I/O priority and keeps running. A job can declare `on_present: kill` instead.

Rec: yes, all of the above. The open question is the arrive-mid-job behavior. The alternative to idle priority is to suspend the job and resume it when the machine goes idle again. That is riskier for builds that hold locks or network connections.

D2n. **D33: harnesses install and update natively.**
- Each harness uses the vendor's official install, a standalone binary wherever one is offered, and its own updater. We never pin or repackage a harness.
- Updates never touch a running sleeve: the next launch or resume uses the new binary.
- The hub runs a harness's update command only when no sleeve of that harness is mid-turn.
- `doctor` records the version and install method, so a behavior change can be traced to a version.
- A failed launch after an update marks that harness ineligible until a launch succeeds, and resleeve skips it.

Rec: yes.

D3n. **D34: Pi runs as its standalone binary, not the npm package, and the hub keeps it current.** The hub checks Pi's GitHub releases every 6 hours, verifies the checksum and stages the new version beside the old one. If Pi's own updater handles the binary install, we use that instead. Rec: yes. To confirm in stage 3: that Pi publishes standalone binaries for linux-arm64 (ms-r1) and checksums with them.

D4n. **D35: Gemini CLI ships through npm, so it is the one harness that brings Node onto the hub.** Rec: accept Node for Gemini only if you actually use Gemini. Otherwise drop Gemini CLI until it ships a standalone binary. Antigravity stays pane-only either way.

## Owner's answers (2026-11-01; removed experiment omitted)

> A1: we are not currently using protectorate at all, you can uninstall and remove it even if you want so dont worry about keeping dev-hotschmoe clean. Its unused
>
> A2: im fine with a limited spike, but also not worried. We can just begin development and as we hit roadblocks we fix them then. Motto is "make it work, make it right, make it fast" so even if we have to kinda hack something together, making it work is priority. We will log where we want to revisit to make things more correct and then revisit to make them fast
>
> A3: i would rather interact mostly via chat (we control design and friendliness on web and mobile) and dropping into the actual terminal should be advanced or like last resort type of thing. Ideally we only interact with envoy, who interacts with each sleeve. We can drop into individual sleeves as we need but really keeping it chat-only would be nice (given this info, this may change our session design, tmux vs other options. We can look how herdr or t3code handles sessions and panes and see if there is anything we want to adopt or learn from. Again though keep it lightweight and minimal dependencies)
>
> A4: so i dont want to give access to cloud agents directly to the hub via token or something. When i have grok bot open on my windows desktop, i can run commands on the desktop which would allow if to run CLI commands for protectorate, and ideally it would just talk to envoy directly. Im unsure about chatgt dot or claude duet but im sure theyre the same. BUT my plan for now is to install grok bot on the same machine as the hub, login to it, and direct it to talk to envoy (on the same machine, no LAN or network access, just all on the hub). So we dont need specific grokbot/duet/dot connectors, they will just use on-machine cli tools via bash or whatever
>
> A5: i accept
>
> A6: i agree
>
> A7: i dont really want to deal with a signing key, is there a way to not make this required? Is the signing key just to ensure the github release binary is exact compared to a local build (no injected code via remote build process). Can we build locally and place those in the release on github? Im a bit unfamiliar with best pratices for release cycles on github
>
> A8: perfect this works
>
> B: yes to all
>
> C: C3- i answered how i view it working in A4, but we could do an MCP and no same machine install if we could do a narrow scope mcp that allowed grokbot/duet/whoever to only talk to envoy. I kinda want to keep it narrow that way. Envoy sleeve should carry ability to handle nodes/sleeves/job/etc and vendor general agents should just be able to talk to envoy to get updates, ask for envoy to do things (maybe envoy can ask permission in the phone app if a general agent asks it do something big/heavy/dangerous?)
> I agree to the rest
>
> D1n: agree
>
> D2n: agree
>
> D3n: every 24 hours is fine to check, better if pi can update itself via ```{harness} update``` hopefully works on all harnesses?
>
> D4n: use the antigravity cli (i think gemini was deprecated, hopefully antigravity has a binary we can install and not rely on node)
>
> Overall we want to keep dependencies to minimum, supply chain attacks are INCREASING at a fast rate, which is why i usually stay away from python and node in important products (quick temporary scripts are fine). And i like zig which has large std and usually small dependencies. Rust usually favors importing a bazillion crates but we dont want to do that, even cargo and crates have had supply chain attacks. If we 100% need a dependency, its almost better to either pin a release (so no updates unless we hand pick them) or fork the dependency ourselves and keep it under our control (and bonus we can trim up forked depednancies)
>
> Small and fast binaries are our bread and butter, rust and zig excel at this. GO where we need (headscale i would feel better using official headscale (built locally) instead of hacking together something with rust or zig but if there is another options im all ears). Flutter i know is a bit heavy, if we have issues we'll move to a lighter GUI but for now flutter's cross platform will aid in development speed. We can optimize UI specifically later
>
> -- i think we are ready to start development. document these decisions, commit, and lets get ripping! you have access to a few machines on the network right now you can test node (and send jobs via ssh to compile on other targets if needed). you have permissions and the next 30 hours to work uninterupted so have fun! i will be available via this chat so if you need something or have questions let me know. dispatch opus/sonnet/grok/codex agents to implement and optimize (and take care that we keep dev loop fast, i dont want a) our disk space exploding b) agents waiting forever on builds so take care you keep machine health under watch and optimize our build and ci to be fast and light and robust
>
> (a clarification, we can have many crates, we want to MINIMIZE crates *we dont own* does that make sense?)

## How the answers landed in SPEC.md

- A1: dev-hotschmoe imposes no compatibility constraint (section 14).
- A2: no spike (D31). The motto and `REVISIT.md` are at the top of SPEC.md.
- A3: chat-first (D38). Headless is the default; terminal takeover is on demand (sections 3.3 and 9).
- A4/C3: general agents run on the hub and talk only to Envoy. Envoy asks the owner before big work. D6 is dropped; D9 is decided (section 3.1).
- A7: no signing key (D36). Releases are built locally and uploaded with `gh` (section 6).
- D3n: harnesses are checked for updates every 24 h, using their own update commands. On this hub, all four (claude, codex, grok, pi) have one.
- D4n: Gemini CLI is dropped. Antigravity CLI replaces it, only as a standalone binary (D35).
- Dependencies: D37 and principle 6. Our own crates are unlimited; third-party code is pinned, listed in `DEPENDENCIES.md`, and forked and trimmed when large.
