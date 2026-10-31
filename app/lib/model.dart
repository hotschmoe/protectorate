// Plain immutable models of the hub API. JSON keys are snake_case, times are unix epoch
// milliseconds (`*_ms`), sizes are bytes (`*_bytes`), and enums travel as lowercase strings. An
// enum value the client does not know parses as `unknown` instead of failing the whole snapshot.

typedef Json = Map<String, Object?>;

String _s(Json j, String k, [String d = '']) {
  final v = j[k];
  return v is String ? v : d;
}

String? _so(Json j, String k) {
  final v = j[k];
  return v is String ? v : null;
}

int _i(Json j, String k, [int d = 0]) {
  final v = j[k];
  return v is num ? v.toInt() : d;
}

int? _io(Json j, String k) {
  final v = j[k];
  return v is num ? v.toInt() : null;
}

double? _do(Json j, String k) {
  final v = j[k];
  return v is num ? v.toDouble() : null;
}

bool _b(Json j, String k, [bool d = false]) {
  final v = j[k];
  return v is bool ? v : d;
}

bool? _bo(Json j, String k) {
  final v = j[k];
  return v is bool ? v : null;
}

DateTime? _t(Json j, String k) {
  final v = _io(j, k);
  return v == null ? null : DateTime.fromMillisecondsSinceEpoch(v);
}

DateTime _tr(Json j, String k) => _t(j, k) ?? DateTime.fromMillisecondsSinceEpoch(0);

Json _m(Json j, String k) {
  final v = j[k];
  return v is Map<String, Object?> ? v : const {};
}

Json? _mo(Json j, String k) {
  final v = j[k];
  return v is Map<String, Object?> ? v : null;
}

List<T> _l<T>(Json j, String k, T Function(Json) f) {
  final v = j[k];
  if (v is! List<Object?>) return const [];
  return [for (final e in v) if (e is Map<String, Object?>) f(e)];
}

List<String> _ls(Json j, String k) {
  final v = j[k];
  if (v is! List<Object?>) return const [];
  return [for (final e in v) if (e is String) e];
}

T _e<T extends Enum>(List<T> values, Json j, String k, T unknown) {
  final v = j[k];
  for (final e in values) {
    if (e.name == v) return e;
  }
  return unknown;
}

// ---- hub -------------------------------------------------------------------------------------

class HubInfo {
  const HubInfo({required this.name, required this.version, required this.startedAt});

  final String name;
  final String version;
  final DateTime startedAt;

  factory HubInfo.fromJson(Json j) => HubInfo(
    name: _s(j, 'name'),
    version: _s(j, 'version'),
    startedAt: _tr(j, 'started_at_ms'),
  );
}

/// One full snapshot. The hub sends it on connect; `GET /api/events` then streams changes.
class HubState {
  const HubState({
    required this.hub,
    required this.workspaces,
    required this.sleeves,
    required this.approvals,
    required this.nodes,
    required this.jobs,
    required this.quotas,
    required this.events,
  });

  final HubInfo hub;
  final List<Workspace> workspaces;
  final List<Sleeve> sleeves;
  final List<Approval> approvals;
  final List<Node> nodes;
  final List<Job> jobs;
  final List<Quota> quotas;
  final List<HubEvent> events;

  factory HubState.fromJson(Json j) => HubState(
    hub: HubInfo.fromJson(_m(j, 'hub')),
    workspaces: _l(j, 'workspaces', Workspace.fromJson),
    sleeves: _l(j, 'sleeves', Sleeve.fromJson),
    approvals: _l(j, 'approvals', Approval.fromJson),
    nodes: _l(j, 'nodes', Node.fromJson),
    jobs: _l(j, 'jobs', Job.fromJson),
    quotas: _l(j, 'quotas', Quota.fromJson),
    events: _l(j, 'events', HubEvent.fromJson),
  );

  Sleeve? sleeve(String id) {
    for (final s in sleeves) {
      if (s.id == id) return s;
    }
    return null;
  }

  Sleeve? get envoy {
    for (final s in sleeves) {
      if (s.role == SleeveRole.envoy) return s;
    }
    return null;
  }

  Node? node(String id) {
    for (final n in nodes) {
      if (n.id == id) return n;
    }
    return null;
  }

  Quota? quota(Subscription s) {
    for (final q in quotas) {
      if (q.subscription == s) return q;
    }
    return null;
  }

  Approval? approval(String id) {
    for (final a in approvals) {
      if (a.id == id) return a;
    }
    return null;
  }

  List<Approval> get pending => [
    for (final a in approvals)
      if (a.state == ApprovalState.pending) a,
  ];

  int get unreadEvents => events.where((e) => !e.read).length;
}

// ---- workspaces and sleeves ------------------------------------------------------------------

class Workspace {
  const Workspace({required this.id, required this.name, required this.path});

  final String id;
  final String name;
  final String path;

  factory Workspace.fromJson(Json j) =>
      Workspace(id: _s(j, 'id'), name: _s(j, 'name'), path: _s(j, 'path'));
}

enum SleeveRole { envoy, root, child, unknown }

/// SPEC 4, sleeve lifecycle.
enum Lifecycle {
  provisioning,
  launching,
  running,
  recovering,
  resleeving,
  stopping,
  stopped,
  parked,
  removed,
  unknown,
}

/// SPEC 4, activity per incarnation.
enum ActivityState {
  booting,
  idle,
  delivering,
  working,
  blocked,
  interrupting,
  compacting,
  unknown,
  ended,
}

enum Harness { claude, codex, grok, piVendor, piLocal, antigravity, unknown }

Harness _harness(Json j, String k) => switch (_s(j, k)) {
  'claude' => Harness.claude,
  'codex' => Harness.codex,
  'grok' => Harness.grok,
  'pi_vendor' => Harness.piVendor,
  'pi_local' => Harness.piLocal,
  'antigravity' => Harness.antigravity,
  _ => Harness.unknown,
};

extension HarnessWire on Harness {
  String get wire => switch (this) {
    Harness.piVendor => 'pi_vendor',
    Harness.piLocal => 'pi_local',
    _ => name,
  };
}

Harness harnessFromWire(String s) => _harness({'h': s}, 'h');

enum Mode { headless, pane, unknown }

/// `Idle{seen}` and `Delivering{id}` carry their payload beside `state`.
class Activity {
  const Activity({required this.state, required this.since, this.seen = true, this.mailId});

  final ActivityState state;
  final DateTime since;
  final bool seen;
  final String? mailId;

  factory Activity.fromJson(Json j) => Activity(
    state: _e(ActivityState.values, j, 'state', ActivityState.unknown),
    since: _tr(j, 'since_ms'),
    seen: _b(j, 'seen', true),
    mailId: _so(j, 'mail_id'),
  );
}

class Incarnation {
  const Incarnation({
    required this.g,
    required this.mode,
    required this.harness,
    required this.subscription,
    required this.provider,
    required this.model,
    required this.activity,
    required this.startedAt,
    required this.turn,
    required this.contextTokens,
    required this.contextWindow,
    this.cacheRatio,
  });

  final int g;
  final Mode mode;
  final Harness harness;
  final Subscription subscription;
  final String provider;
  final String model;
  final Activity activity;
  final DateTime startedAt;
  final int turn;
  final int contextTokens;
  final int contextWindow;
  final double? cacheRatio;

  factory Incarnation.fromJson(Json j) => Incarnation(
    g: _i(j, 'g'),
    mode: _e(Mode.values, j, 'mode', Mode.unknown),
    harness: _harness(j, 'harness'),
    subscription: _e(Subscription.values, j, 'subscription', Subscription.unknown),
    provider: _s(j, 'provider'),
    model: _s(j, 'model'),
    activity: Activity.fromJson(_m(j, 'activity')),
    startedAt: _tr(j, 'started_at_ms'),
    turn: _i(j, 'turn'),
    contextTokens: _i(j, 'context_tokens'),
    contextWindow: _i(j, 'context_window'),
    cacheRatio: _do(j, 'cache_ratio'),
  );

  double? get contextFraction => contextWindow > 0 ? contextTokens / contextWindow : null;
}

class Sleeve {
  const Sleeve({
    required this.id,
    required this.name,
    required this.workspaceId,
    required this.role,
    required this.lifecycle,
    required this.resleeveTo,
    this.parentId,
    this.branch,
    this.incarnation,
    this.budgetUsed,
    this.budgetLimit,
  });

  final String id;
  final String name;
  final String workspaceId;
  final String? parentId;
  final SleeveRole role;
  final Lifecycle lifecycle;
  final List<Harness> resleeveTo;
  final String? branch;
  final Incarnation? incarnation;
  final int? budgetUsed;
  final int? budgetLimit;

  factory Sleeve.fromJson(Json j) {
    final inc = _mo(j, 'incarnation');
    return Sleeve(
      id: _s(j, 'id'),
      name: _s(j, 'name'),
      workspaceId: _s(j, 'workspace_id'),
      parentId: _so(j, 'parent_id'),
      role: _e(SleeveRole.values, j, 'role', SleeveRole.unknown),
      lifecycle: _e(Lifecycle.values, j, 'lifecycle', Lifecycle.unknown),
      resleeveTo: [for (final h in _ls(j, 'resleeve_to')) harnessFromWire(h)],
      branch: _so(j, 'branch'),
      incarnation: inc == null ? null : Incarnation.fromJson(inc),
      budgetUsed: _io(j, 'budget_used'),
      budgetLimit: _io(j, 'budget_limit'),
    );
  }

  ActivityState? get activity => incarnation?.activity.state;
}

// ---- transcript ------------------------------------------------------------------------------

enum EntryKind { user, assistant, tool, approval, notice, unknown }

enum ToolStatus { running, ok, error, unknown }

enum Level { info, warn, alarm }

/// One line of a sleeve's chat, built by the hub from the headless transcript. `kind` picks the
/// fields that apply.
class ChatEntry {
  const ChatEntry({
    required this.seq,
    required this.at,
    required this.g,
    required this.kind,
    this.from = '',
    this.text = '',
    this.streaming = false,
    this.callId = '',
    this.tool = '',
    this.input = '',
    this.output = '',
    this.status = ToolStatus.unknown,
    this.endedAt,
    this.approvalId = '',
    this.level = Level.info,
  });

  final int seq;
  final DateTime at;
  final int g;
  final EntryKind kind;

  /// user: `owner`, `envoy` or the sending sleeve's name (mail).
  final String from;

  /// user, assistant, notice: the text. tool: the one-line summary.
  final String text;
  final bool streaming;

  final String callId;
  final String tool;
  final String input;
  final String output;
  final ToolStatus status;
  final DateTime? endedAt;

  final String approvalId;
  final Level level;

  factory ChatEntry.fromJson(Json j) => ChatEntry(
    seq: _i(j, 'seq'),
    at: _tr(j, 'at_ms'),
    g: _i(j, 'g'),
    kind: _e(EntryKind.values, j, 'kind', EntryKind.unknown),
    from: _s(j, 'from'),
    text: _s(j, 'text'),
    streaming: _b(j, 'streaming'),
    callId: _s(j, 'call_id'),
    tool: _s(j, 'tool'),
    input: _s(j, 'input'),
    output: _s(j, 'output'),
    status: _e(ToolStatus.values, j, 'status', ToolStatus.unknown),
    endedAt: _t(j, 'ended_at_ms'),
    approvalId: _s(j, 'approval_id'),
    level: _e(Level.values, j, 'level', Level.info),
  );
}

class Transcript {
  const Transcript({required this.sleeveId, required this.entries});

  final String sleeveId;
  final List<ChatEntry> entries;

  factory Transcript.fromJson(Json j) =>
      Transcript(sleeveId: _s(j, 'sleeve_id'), entries: _l(j, 'entries', ChatEntry.fromJson));
}

// ---- approvals -------------------------------------------------------------------------------

/// `tool` is a harness permission request; `action` is one the hub's code rule requires
/// (removing sleeves, node maintenance jobs, pushes to protected branches).
enum ApprovalKind { tool, action, unknown }

enum ApprovalState { pending, approved, denied, expired, unknown }

class Approval {
  const Approval({
    required this.id,
    required this.sleeveId,
    required this.kind,
    required this.title,
    required this.summary,
    required this.detail,
    required this.requestedAt,
    required this.state,
    this.timeoutAt,
    this.decidedBy,
    this.decidedAt,
  });

  final String id;
  final String sleeveId;
  final ApprovalKind kind;
  final String title;
  final String summary;
  final String detail;
  final DateTime requestedAt;
  final DateTime? timeoutAt;
  final ApprovalState state;
  final String? decidedBy;
  final DateTime? decidedAt;

  factory Approval.fromJson(Json j) => Approval(
    id: _s(j, 'id'),
    sleeveId: _s(j, 'sleeve_id'),
    kind: _e(ApprovalKind.values, j, 'kind', ApprovalKind.unknown),
    title: _s(j, 'title'),
    summary: _s(j, 'summary'),
    detail: _s(j, 'detail'),
    requestedAt: _tr(j, 'requested_at_ms'),
    timeoutAt: _t(j, 'timeout_at_ms'),
    state: _e(ApprovalState.values, j, 'state', ApprovalState.unknown),
    decidedBy: _so(j, 'decided_by'),
    decidedAt: _t(j, 'decided_at_ms'),
  );
}

// ---- nodes and jobs --------------------------------------------------------------------------

enum PresenceState { present, away, unknown }

class Presence {
  const Presence({
    required this.state,
    required this.idleSecs,
    required this.source,
    required this.sampledAt,
    this.locked,
  });

  final PresenceState state;
  final int? idleSecs;
  final bool? locked;
  final String source;
  final DateTime sampledAt;

  factory Presence.fromJson(Json j) => Presence(
    state: _e(PresenceState.values, j, 'state', PresenceState.unknown),
    idleSecs: _io(j, 'idle_secs'),
    locked: _bo(j, 'locked'),
    source: _s(j, 'source'),
    sampledAt: _tr(j, 'sampled_at_ms'),
  );
}

class Gpu {
  const Gpu({required this.name, required this.driver, this.vramBytes, this.utilPct});

  final String name;
  final String driver;
  final int? vramBytes;
  final double? utilPct;

  factory Gpu.fromJson(Json j) => Gpu(
    name: _s(j, 'name'),
    driver: _s(j, 'driver'),
    vramBytes: _io(j, 'vram_bytes'),
    utilPct: _do(j, 'util_pct'),
  );
}

class Disk {
  const Disk({required this.mount, required this.sizeBytes, required this.freeBytes});

  final String mount;
  final int sizeBytes;
  final int freeBytes;

  factory Disk.fromJson(Json j) => Disk(
    mount: _s(j, 'mount'),
    sizeBytes: _i(j, 'size_bytes'),
    freeBytes: _i(j, 'free_bytes'),
  );

  double get usedFraction => sizeBytes > 0 ? 1 - freeBytes / sizeBytes : 0;
}

class Toolchain {
  const Toolchain({required this.name, required this.version});

  final String name;
  final String version;

  factory Toolchain.fromJson(Json j) =>
      Toolchain(name: _s(j, 'name'), version: _s(j, 'version'));
}

class Inventory {
  const Inventory({
    required this.hostname,
    required this.os,
    required this.osVersion,
    required this.arch,
    required this.cpuModel,
    required this.cpuCount,
    required this.ramBytes,
    required this.gpus,
    required this.disks,
    required this.toolchains,
    required this.collectedAt,
  });

  final String hostname;
  final String os;
  final String osVersion;
  final String arch;
  final String cpuModel;
  final int cpuCount;
  final int ramBytes;
  final List<Gpu> gpus;
  final List<Disk> disks;
  final List<Toolchain> toolchains;
  final DateTime collectedAt;

  factory Inventory.fromJson(Json j) => Inventory(
    hostname: _s(j, 'hostname'),
    os: _s(j, 'os'),
    osVersion: _s(j, 'os_version'),
    arch: _s(j, 'arch'),
    cpuModel: _s(j, 'cpu_model'),
    cpuCount: _i(j, 'cpu_count'),
    ramBytes: _i(j, 'ram_bytes'),
    gpus: _l(j, 'gpus', Gpu.fromJson),
    disks: _l(j, 'disks', Disk.fromJson),
    toolchains: _l(j, 'toolchains', Toolchain.fromJson),
    collectedAt: _tr(j, 'collected_at_ms'),
  );
}

/// Unknown numbers are null and never count as spare capacity.
class Sample {
  const Sample({
    required this.sampledAt,
    this.cpuPct,
    this.load1,
    this.memAvailBytes,
    this.swapUsedBytes,
  });

  final DateTime sampledAt;
  final double? cpuPct;
  final double? load1;
  final int? memAvailBytes;
  final int? swapUsedBytes;

  factory Sample.fromJson(Json j) => Sample(
    sampledAt: _tr(j, 'sampled_at_ms'),
    cpuPct: _do(j, 'cpu_pct'),
    load1: _do(j, 'load1'),
    memAvailBytes: _io(j, 'mem_avail_bytes'),
    swapUsedBytes: _io(j, 'swap_used_bytes'),
  );
}

class Node {
  const Node({
    required this.id,
    required this.name,
    required this.online,
    required this.lastSeen,
    required this.nodeVersion,
    required this.inventory,
    required this.presence,
    this.sample,
  });

  final String id;
  final String name;
  final bool online;
  final DateTime lastSeen;
  final String nodeVersion;
  final Inventory inventory;
  final Sample? sample;
  final Presence presence;

  factory Node.fromJson(Json j) {
    final s = _mo(j, 'sample');
    return Node(
      id: _s(j, 'id'),
      name: _s(j, 'name'),
      online: _b(j, 'online'),
      lastSeen: _tr(j, 'last_seen_ms'),
      nodeVersion: _s(j, 'node_version'),
      inventory: Inventory.fromJson(_m(j, 'inventory')),
      sample: s == null ? null : Sample.fromJson(s),
      presence: Presence.fromJson(_m(j, 'presence')),
    );
  }

  double? get memUsedFraction {
    final a = sample?.memAvailBytes;
    final t = inventory.ramBytes;
    return a == null || t <= 0 ? null : 1 - a / t;
  }
}

/// SPEC 4, node job.
enum JobState {
  queued,
  submitting,
  submitUncertain,
  running,
  collecting,
  succeeded,
  failed,
  interrupted,
  lost,
  cancelling,
  cancelled,
  abandoned,
  unknown,
}

JobState _jobState(String s) => switch (s) {
  'submit_uncertain' => JobState.submitUncertain,
  _ => JobState.values.firstWhere((e) => e.name == s, orElse: () => JobState.unknown),
};

class Job {
  const Job({
    required this.id,
    required this.nodeId,
    required this.state,
    required this.command,
    required this.requestedBy,
    required this.attempt,
    required this.queuedAt,
    this.startedAt,
    this.endedAt,
    this.exitCode,
  });

  final String id;
  final String nodeId;
  final JobState state;
  final String command;
  final String requestedBy;
  final int attempt;
  final DateTime queuedAt;
  final DateTime? startedAt;
  final DateTime? endedAt;
  final int? exitCode;

  factory Job.fromJson(Json j) => Job(
    id: _s(j, 'id'),
    nodeId: _s(j, 'node_id'),
    state: _jobState(_s(j, 'state')),
    command: _s(j, 'command'),
    requestedBy: _s(j, 'requested_by'),
    attempt: _i(j, 'attempt', 1),
    queuedAt: _tr(j, 'queued_at_ms'),
    startedAt: _t(j, 'started_at_ms'),
    endedAt: _t(j, 'ended_at_ms'),
    exitCode: _io(j, 'exit_code'),
  );
}

// ---- quota -----------------------------------------------------------------------------------

enum Subscription { claude, codex, grok, local, unknown }

enum QuotaReading { ok, stale, error, unknown }

class QuotaWindow {
  const QuotaWindow({required this.label, this.usedPct, this.resetsAt});

  /// `5h`, `wk`, `day` as the vendor names it.
  final String label;
  final double? usedPct;
  final DateTime? resetsAt;

  factory QuotaWindow.fromJson(Json j) => QuotaWindow(
    label: _s(j, 'label'),
    usedPct: _do(j, 'used_pct'),
    resetsAt: _t(j, 'resets_at_ms'),
  );
}

class Quota {
  const Quota({
    required this.subscription,
    required this.plan,
    required this.reading,
    required this.fetchedAt,
    required this.windows,
    this.note = '',
  });

  final Subscription subscription;
  final String plan;
  final QuotaReading reading;
  final DateTime fetchedAt;
  final List<QuotaWindow> windows;
  final String note;

  factory Quota.fromJson(Json j) => Quota(
    subscription: _e(Subscription.values, j, 'subscription', Subscription.unknown),
    plan: _s(j, 'plan'),
    reading: _e(QuotaReading.values, j, 'reading', QuotaReading.unknown),
    fetchedAt: _tr(j, 'fetched_at_ms'),
    windows: _l(j, 'windows', QuotaWindow.fromJson),
    note: _s(j, 'note'),
  );

  /// The tightest window, the one the quota hint shows.
  QuotaWindow? get tightest {
    QuotaWindow? best;
    for (final w in windows) {
      if (w.usedPct == null) continue;
      if (best == null || w.usedPct! > best.usedPct!) best = w;
    }
    return best;
  }
}

// ---- mail and events -------------------------------------------------------------------------

enum EventKind { mail, sleeve, approval, job, node, guardrail, hub, unknown }

class HubEvent {
  const HubEvent({
    required this.seq,
    required this.at,
    required this.kind,
    required this.source,
    required this.text,
    required this.read,
    this.target,
    this.level = Level.info,
  });

  final int seq;
  final DateTime at;
  final EventKind kind;
  final String source;
  final String? target;
  final String text;
  final bool read;
  final Level level;

  factory HubEvent.fromJson(Json j) => HubEvent(
    seq: _i(j, 'seq'),
    at: _tr(j, 'at_ms'),
    kind: _e(EventKind.values, j, 'kind', EventKind.unknown),
    source: _s(j, 'source'),
    target: _so(j, 'target'),
    text: _s(j, 'text'),
    read: _b(j, 'read'),
    level: _e(Level.values, j, 'level', Level.info),
  );
}
