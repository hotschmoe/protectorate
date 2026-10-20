#!/usr/bin/env python3
"""Render the review inventory; never modify or remove test sources."""
import argparse
import collections
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
SCENARIOS = {
    "auth": "AUTH: scoped bearer, denial, revoke, expiry, audit and restart",
    "sleeve": "SLEEVE: real sandbox lifecycle, secret boundaries, symlinks, git clone, OOM and toolchains",
    "mailbox": "MAIL: authenticated send/reply/ack, idle delivery, FIFO notices and restart",
    "directory": "MAIL: real repositories, project resolution and dependency notices",
    "driver": "CHAT: official protocol subprocesses, approval, interruption, compaction and replay",
    "chatlog": "CHAT: live stream versus restart replay, partial writes and slow consumer reconnect",
    "handoff": "HANDOFF: real two-harness resleeve with git facts and persisted handoff",
    "node": "NODE: real agent install/update/rollback, file confinement and transfer",
    "backup": "BACKUP: live hub snapshot, restore, tamper rejection and headscale integrity",
    "session": "TERMINAL: real tmux lifecycle, WebSocket I/O, exit and respawn",
    "tailnet": "TAILNET: isolated headscale, two nodes, ACL, relay and restart",
    "quota": "QUOTA: hub polls local provider HTTP fixtures and exposes freshness/errors",
    "desk": "DESK: isolated node desktop consent, lease, revocation and input fencing",
    "orchestrator": "MAIL: Envoy identity, wakeup, grants and mailbox delivery",
    "mcpbridge": "MCP: binary stdio handshake, tools, scopes and streamed errors through hub",
    "backlog": "BACKLOG: authenticated CRUD, concurrent writes and hub restart persistence",
    "xfer": "TRANSFER: binary hub/node chunks, compression, SHA-256 and malicious archive rejection",
    "keylog": "DESK: real input activity monitoring gates desktop consent and revocation",
    "svcgen": "INSTALL: generated service definitions, offline systemd verification and isolated service lifecycle",
    "fsutil": "BACKUP: permission-preserving atomic persistence and interrupted writes",
    "web": "WEB: binary serves SPA, bootstrap, pairing, install scripts and downloads",
}
KEEP = set("""
TestNewSession
TestEventDetails
TestResume
TestApproval
TestInterruptAndSteer
TestExitMidTurn
TestCodexResumeLocked
TestStderrBounded
TestCloseEscalates
TestOwnersOpenDriver
TestCommandWrapsHarness
TestUsageAndToolResults
TestThinkingAndMessageIDs
TestCompact
TestCompactBusy
TestNativeAutoCompaction
TestAutonomousFlags
TestCodexAutonomousLeavesPolicyToConfig
TestSyntheticMessageReportsNoUsage
TestMain
TestCodexSelectionProtocol
TestSelectionUnsupportedAndNativeDefaults
TestCodexSelectionAtOpenAndResume
TestSelectionUsesNativeConfirmation
TestChatEndToEnd
TestChatShellAndMissing
TestChatSupervisedPermissionMode
TestChatTerminalSleeveHasNoChat
TestSleeveTokenReadsOnlyItself
TestChatSendRequestIDsSurviveHubRestart
TestMailWaitsForIdleAndIsAuthenticated
TestMailReplyKeepsThreadAndAcks
TestMailSendValidationAndDedupe
TestMailSurvivesRestartAndTerminalSleevesUseInbox
TestMailEscalation
TestMailProjectAddressing
TestLeaseGrantSendsNotice
TestMailRequestAcksByDefault
TestMailListThreads
TestDeskEndedNotice
TestEnvoyMailQueue
TestEnvoyMailAfterOwner
TestMailWakeWaitE2E
TestSleeveReadOnlyMountHTTP
TestSleeveIdleReapHTTP
TestDevelopmentToolchainsEndToEnd
TestGitRunsRepositoryProgramsInAJail
TestSleeveLifecycle
TestMemoryLimitEnforced
TestSleeveExitKeepsSession
TestSleeveExitReasons
TestSpawnClonesGitURL
TestKillRestoresRecordWhenSaveFails
TestBackupRestoreWithLiveHeadscale
TestTailnetEndToEnd
TestNodeRestartRecovery
TestDERPRelay
TestBinaryClusterRestartAndRestore
TestBinaryWorkspaceBrowser
TestBinaryJobArtifactsStayInWorkspace
""".split())
DELETE = set()

def scenario(path, name):
    lower = str(path).lower() + name.lower()
    if str(path).startswith("app/"):
        return "APP: Flutter client against real loopback hub; " + ("chat live/replay equivalence and reconnect" if "chat" in lower else re.sub(r"_test\.dart$", "", path.name).replace("_", " "))
    if "/hub/" in lower or "cmd/protectorate" in lower:
        for key, value in [("lease", "LEASE: exclusivity, FIFO, expiry/renewal and restart"), ("job", "JOB: real agent git/artifact execution, cancel, idempotency and hub restart"), ("mail", SCENARIOS["mailbox"]), ("coord", SCENARIOS["mailbox"]), ("grant", SCENARIOS["auth"]), ("token", SCENARIOS["auth"]), ("auth", SCENARIOS["auth"]), ("chat", SCENARIOS["driver"]), ("sleeve", SCENARIOS["sleeve"]), ("envoy", SCENARIOS["orchestrator"]), ("resleeve", SCENARIOS["handoff"]), ("node", SCENARIOS["node"]), ("transfer", SCENARIOS["node"]), ("desk", SCENARIOS["desk"]), ("session", SCENARIOS["session"]), ("tailnet", SCENARIOS["tailnet"]), ("quota", SCENARIOS["quota"]), ("mcp", SCENARIOS["mcpbridge"]), ("install", SCENARIOS["web"]), ("web", SCENARIOS["web"]), ("backlog", "BACKLOG: authenticated CRUD, persistence and concurrent updates")]:
            if key in lower:
                return value
        return "CLI: real binary API commands, JSON output, cancellation and error exit codes"
    return SCENARIOS.get(path.parts[1] if len(path.parts) > 1 else "", "SYSTEM: real binary configuration, status and persistence")

def purpose(name):
    return re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", re.sub(r"^(Test|Fuzz|Example)", "", name)).replace("|", "\\|")

def collect(results):
    timings = {}
    for result in results:
        for line in result.read_text().splitlines():
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if event.get("Action") in {"pass", "fail", "skip"} and "/" not in event.get("Test", ""):
                timings[(event.get("Package", "").removeprefix("github.com/hotschmoe/protectorate/"), event.get("Test"))] = (event.get("Elapsed", 0), event["Action"])
    rows = []
    for absolute in sorted(ROOT.rglob("*_test.go")):
        path = absolute.relative_to(ROOT)
        if any(part in {"archive", "references", "gen", ".git"} for part in path.parts):
            continue
        source = absolute.read_text()
        for match in re.finditer(r"^func ((?:Test|Fuzz|Example)\w*)\s*\(", source, re.M):
            name = match[1]
            if name == "TestMain":
                verdict, replacement = "KEEP-E2E", "runner infrastructure; retain while any package tests use it"
            elif name.startswith("Fuzz"):
                verdict, replacement = "KEEP-FUZZ", "seed corpus on commits; bounded campaigns separately"
            elif name in KEEP:
                verdict, replacement = "KEEP-E2E", "existing real process/HTTP workflow; do not delete without replacement"
            elif name in DELETE:
                verdict, replacement = "DELETE", "implementation/output-shape assertion; validate externally visible behavior in " + scenario(path, name)
            else:
                verdict, replacement = "CONVERT", scenario(path, name) + "; preserve the named behavior as a scenario assertion"
            timing = timings.get((str(path.parent), name))
            runtime = f"{timing[0]:.3f}s {timing[1]}" if timing else "not measured"
            rows.append((str(path), name, purpose(name), runtime, verdict, replacement))
    for absolute in sorted((ROOT / "app/test").rglob("*.dart")):
        path = absolute.relative_to(ROOT)
        # A registered callback is Dart's test function. Include wrapped literals.
        pattern = r"\b(testWidgets|test)\s*\(\s*((?:r?[\"'][^\n]*?[\"']\s*)+),"
        source = absolute.read_text()
        literals = {match.start(): match for match in re.finditer(pattern, source)}
        for registration in re.finditer(r"\b(testWidgets|test)\s*\(\s*([^,]+),", source):
            match = literals.get(registration.start())
            if match:
                label = " ".join(re.findall(r"r?[\"'](.*?)[\"']", match[2])).replace("|", "\\|")
            else:
                label = "table callback: " + registration[2].strip().replace("|", "\\|")
            line = source[:registration.start()].count("\n") + 1
            rows.append((str(path), f"{registration[1]}: {label} (L{line})", label, "not measured: Flutter unavailable", "CONVERT", scenario(path, label)))
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--go-results", type=pathlib.Path, action="append", default=[])
    args = parser.parse_args()
    rows = collect(args.go_results)
    counts = collections.Counter(row[4] for row in rows)
    preamble = (ROOT / "docs/testing-policy.md").read_text()
    out = [preamble, "\n## Inventory\n", "Snapshot counts: " + ", ".join(f"{key}={value}" for key, value in sorted(counts.items())) + f"; total={len(rows)}. TestMain rows are infrastructure, not scenarios.\n", "Runtime is the observed non-race test-function elapsed time, including skips. It excludes build time and is not a race-suite prediction. Unknown means unmeasured, never zero. Dart callback labels include source lines to distinguish repeated labels.\n", "| File | Function / callback | Exercises | Runtime | Verdict | Replacement / reason |", "|---|---|---|---|---|---|"]
    out.extend("| " + " | ".join(row) + " |" for row in rows)
    out.extend(["\n## Deleted files\n", "See [the phase 2 deletion record](testing-deletions.md) for the exact paths grouped by package."])
    (ROOT / "docs/testing.md").write_text("\n".join(out) + "\n")
    print(f"Inventoried {len(rows)} functions/callbacks: {dict(counts)}")

if __name__ == "__main__":
    main()
