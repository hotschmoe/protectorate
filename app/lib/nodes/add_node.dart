import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:qr/qr.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/hub_client.dart';
import '../screens/hub_shell.dart' show phoneBreakpoint;
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';

// The hub's rule for invite names: a tailnet machine name (DNS label).
final _nameRe = RegExp(r'^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$');

String? nodeNameError(String name) {
  if (name.isEmpty) return null;
  if (name.length > 63) return 'At most 63 characters.';
  if (!_nameRe.hasMatch(name)) {
    return 'Lowercase letters, digits and -, not at either end.';
  }
  return null;
}

// The machine the app runs on is the likeliest new node. On the web
// defaultTargetPlatform comes from the browser's user agent.
InstallPlatform platformFor(TargetPlatform p) => switch (p) {
  TargetPlatform.windows => InstallPlatform.INSTALL_PLATFORM_WINDOWS,
  TargetPlatform.android => InstallPlatform.INSTALL_PLATFORM_TERMUX,
  _ => InstallPlatform.INSTALL_PLATFORM_LINUX,
};

const installPlatforms = [
  (InstallPlatform.INSTALL_PLATFORM_WINDOWS, 'WINDOWS'),
  (InstallPlatform.INSTALL_PLATFORM_LINUX, 'LINUX'),
  (InstallPlatform.INSTALL_PLATFORM_TERMUX, 'TERMUX'),
];

const _platformHint = {
  InstallPlatform.INSTALL_PLATFORM_WINDOWS:
      'x64 or ARM64, Windows PowerShell 5.1 or pwsh. Installs a logon task for the signed-in user; no administrator rights.',
  InstallPlatform.INSTALL_PLATFORM_LINUX:
      'x86_64 or aarch64 with curl. Installs a systemd user unit.',
  InstallPlatform.INSTALL_PLATFORM_TERMUX:
      'Android aarch64 in Termux. Starts at boot with termux-services and Termux:Boot.',
};

const setupStages = [
  (NodeSetupStage.NODE_SETUP_STAGE_INVITED, 'INVITED'),
  (NodeSetupStage.NODE_SETUP_STAGE_JOINED, 'JOINED'),
  (NodeSetupStage.NODE_SETUP_STAGE_METRICS, 'FIRST METRICS'),
  (NodeSetupStage.NODE_SETUP_STAGE_READY, 'READY'),
];

int _stageIndex(NodeSetupProgress p) {
  if (p.stage == NodeSetupStage.NODE_SETUP_STAGE_ATTENTION) {
    if (p.status.hasMetrics()) return 2;
    return p.status.node.invited ? 0 : 1;
  }
  return setupStages.indexWhere((s) => s.$1 == p.stage);
}

String inviteError(Object e) {
  if (e is ConnectException && e.code == Code.permissionDenied) {
    return 'Adding nodes is owner-only. On the hub, run protectorate node invite <name>, then follow its install instructions on the new machine.';
  }
  if (e is ConnectException) return e.message;
  return '$e';
}

Future<void> showAddNode(
  BuildContext context, {
  required HubClient client,
  String initialName = '',
  bool reissue = false,
  TargetPlatform? platform,
  VoidCallback? onChanged,
}) => showDialog<void>(
  context: context,
  barrierDismissible: false,
  builder: (_) => AddNodeDialog(
    client: client,
    initialName: initialName,
    reissue: reissue,
    platform: platform,
    onChanged: onChanged,
  ),
);

class AddNodeDialog extends StatefulWidget {
  const AddNodeDialog({
    super.key,
    required this.client,
    this.initialName = '',
    this.reissue = false,
    this.platform,
    this.onChanged,
  });

  final HubClient client;
  final String initialName;
  // reissue issues a new key for a pending node at once.
  final bool reissue;
  final TargetPlatform? platform;
  final VoidCallback? onChanged;

  @override
  State<AddNodeDialog> createState() => _AddNodeDialogState();
}

class _AddNodeDialogState extends State<AddNodeDialog> {
  late final _name = TextEditingController(text: widget.initialName);
  late InstallPlatform _platform = platformFor(
    widget.platform ?? defaultTargetPlatform,
  );
  final _abort = CancelableSignal();
  InviteNodeResponse? _invite;
  NodeSetupProgress? _progress;
  String? _error;
  bool _busy = false;
  bool _copied = false;
  Timer? _copiedReset;

  @override
  void initState() {
    super.initState();
    if (widget.reissue) _create();
  }

  @override
  void dispose() {
    _abort.cancel();
    _copiedReset?.cancel();
    _name.dispose();
    super.dispose();
  }

  String get _nameText => _name.text.trim();

  bool get _nameOk => _nameText.isNotEmpty && nodeNameError(_nameText) == null;

  Future<void> _create() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final r = await widget.client.node.inviteNode(
        InviteNodeRequest(name: _nameText),
      );
      if (!mounted) return;
      setState(() => _invite = r);
      widget.onChanged?.call();
      _watch();
    } on Object catch (e) {
      if (mounted) setState(() => _error = inviteError(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _watch() {
    widget.client.node
        .watchNodeSetup(
          WatchNodeSetupRequest(name: _invite!.node.name),
          signal: _abort,
        )
        .listen(
          (p) {
            if (!mounted) return;
            setState(() => _progress = p);
            if (p.stage == NodeSetupStage.NODE_SETUP_STAGE_READY) {
              widget.onChanged?.call();
            }
          },
          onError: (Object e) {
            if (mounted) setState(() => _error = inviteError(e));
          },
          cancelOnError: true,
        );
  }

  InstallCommand? get _command {
    for (final c in _invite?.commands ?? const <InstallCommand>[]) {
      if (c.platform == _platform) return c;
    }
    return null;
  }

  Future<void> _copy(String text) async {
    await Clipboard.setData(ClipboardData(text: text));
    if (!mounted) return;
    setState(() => _copied = true);
    _copiedReset?.cancel();
    _copiedReset = Timer(const Duration(seconds: 2), () {
      if (mounted) setState(() => _copied = false);
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final ready = _progress?.stage == NodeSetupStage.NODE_SETUP_STAGE_READY;
    final phone = MediaQuery.sizeOf(context).width < phoneBreakpoint;
    return AlertDialog(
      title: Text(widget.reissue ? 'Reissue key' : 'Add node'),
      content: SizedBox(
        width: 560,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_invite == null) ..._form(t) else ..._issued(t, phone),
              if (_error != null) ...[
                const SizedBox(height: Tokens.gap),
                NoteStrip(_error!, tone: Tone.alarm),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('add-close'),
          onPressed: () => Navigator.of(context).pop(),
          child: Text(ready ? 'DONE' : 'CLOSE'),
        ),
        if (_invite == null)
          FilledButton(
            key: const ValueKey('add-create'),
            onPressed: _nameOk && !_busy ? _create : null,
            child: const Text('CREATE INVITE'),
          ),
      ],
    );
  }

  List<Widget> _form(Tokens t) => [
    TextField(
      key: const ValueKey('add-name'),
      controller: _name,
      autofocus: true,
      autocorrect: false,
      enableSuggestions: false,
      style: t.data(t.p.emph),
      inputFormatters: [
        FilteringTextInputFormatter.deny(RegExp(r'\s')),
        TextInputFormatter.withFunction(
          (_, v) => v.copyWith(text: v.text.toLowerCase()),
        ),
      ],
      decoration: InputDecoration(
        labelText: 'Node name',
        helperText: 'How the hub and agents will call it, e.g. office-arm',
        errorText: nodeNameError(_nameText),
      ),
      onChanged: (_) => setState(() {}),
      onSubmitted: (_) {
        if (_nameOk && !_busy) _create();
      },
    ),
    const SizedBox(height: Tokens.gapL),
    Text('MACHINE', style: t.legend()),
    const SizedBox(height: 4),
    ChoiceKeys<InstallPlatform>(
      key: const ValueKey('add-platform'),
      options: installPlatforms,
      selected: _platform,
      onSelected: (p) => setState(() => _platform = p),
    ),
    const SizedBox(height: 4),
    Text(_platformHint[_platform] ?? '', style: t.small()),
    const SizedBox(height: Tokens.gap),
    if (_busy)
      const BusyBar()
    else
      const NoteStrip(
        'The hub issues a single-use key for this name, valid for one hour, and shows the line to paste on the new machine. Anyone with the line can enroll a machine until it is used.',
        tone: Tone.info,
      ),
  ];

  List<Widget> _issued(Tokens t, bool phone) {
    final inv = _invite!;
    final cmd = _command;
    final exp = inv.expiresAt.toDateTime().toLocal();
    String two(int v) => v.toString().padLeft(2, '0');
    return [
      Row(
        children: [
          Expanded(child: Text(inv.node.name, style: t.title())),
          Text(
            'KEY UNTIL ${two(exp.hour)}:${two(exp.minute)}',
            style: t.legend(),
          ),
        ],
      ),
      const SizedBox(height: Tokens.gapS),
      ChoiceKeys<InstallPlatform>(
        key: const ValueKey('add-platform'),
        options: installPlatforms,
        selected: _platform,
        onSelected: (p) => setState(() => _platform = p),
      ),
      const SizedBox(height: Tokens.gapS),
      if (cmd != null)
        Panel(
          key: const ValueKey('add-command'),
          legend: 'PASTE INTO ${cmd.shell.toUpperCase()}',
          trailing: TextButton.icon(
            key: const ValueKey('add-copy'),
            onPressed: () => _copy(cmd.command),
            icon: Icon(_copied ? Icons.check : Icons.copy, size: 16),
            label: Text(_copied ? 'COPIED' : 'COPY'),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SelectableText(
                cmd.command,
                key: const ValueKey('add-command-text'),
                style: t.data(t.p.emph),
              ),
              const SizedBox(height: Tokens.gapS),
              Text(cmd.note, style: t.small()),
              if (phone) ...[
                const SizedBox(height: Tokens.gap),
                Center(
                  child: QrBlock(
                    key: const ValueKey('add-qr'),
                    data: cmd.command,
                  ),
                ),
              ],
            ],
          ),
        ),
      const SizedBox(height: Tokens.gap),
      SetupPanel(progress: _progress),
    ];
  }
}

// The live path of a new node: one lamp per stage, the current one working,
// problems below.
class SetupPanel extends StatelessWidget {
  const SetupPanel({super.key, required this.progress});

  final NodeSetupProgress? progress;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final p = progress;
    final at = p == null ? 0 : _stageIndex(p);
    final attention = p?.stage == NodeSetupStage.NODE_SETUP_STAGE_ATTENTION;
    final ready = p?.stage == NodeSetupStage.NODE_SETUP_STAGE_READY;
    return Panel(
      key: const ValueKey('add-setup'),
      legend: 'SETUP',
      tone: attention ? Tone.warn : null,
      trailing: ready
          ? const LampLegend(label: 'IN SERVICE', tone: Tone.ok)
          : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: Tokens.gapL,
            runSpacing: Tokens.gapS,
            children: [
              for (var i = 0; i < setupStages.length; i++)
                LampLegend(
                  key: ValueKey('stage-${setupStages[i].$2}'),
                  label: setupStages[i].$2,
                  tone: i < at || ready
                      ? Tone.ok
                      : i == at
                      ? (attention ? Tone.warn : Tone.info)
                      : Tone.ghost,
                  on: i <= at || ready,
                ),
            ],
          ),
          const SizedBox(height: Tokens.gapS),
          if (!ready && !attention) const BusyBar(tone: Tone.info),
          if (p != null && p.message.isNotEmpty) ...[
            const SizedBox(height: Tokens.gapS),
            Text(
              p.message,
              key: const ValueKey('setup-message'),
              style: t.small(),
            ),
          ],
          for (final problem in p?.problems ?? const <String>[])
            Padding(
              padding: const EdgeInsets.only(top: Tokens.gapS),
              child: NoteStrip(problem),
            ),
          for (final note in p?.notes ?? const <String>[])
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(note, style: t.small(t.p.legend)),
            ),
        ],
      ),
    );
  }
}

// A QR code of data, dark modules on the light end of the palette in both
// themes, as scanners expect.
class QrBlock extends StatelessWidget {
  const QrBlock({super.key, required this.data, this.size = 220});

  final String data;
  final double size;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final light = t.dark ? t.p.emph : t.p.glass;
    final dark = t.dark ? t.p.glass : t.p.emph;
    final image = QrImage(
      QrCode.fromData(data: data, errorCorrectLevel: QrErrorCorrectLevel.L),
    );
    return Semantics(
      label: 'QR code of the install command',
      child: CustomPaint(
        size: Size.square(size),
        painter: _QrPainter(image, light: light, dark: dark),
      ),
    );
  }
}

class _QrPainter extends CustomPainter {
  _QrPainter(this.image, {required this.light, required this.dark});

  final QrImage image;
  final Color light;
  final Color dark;

  static const _quiet = 4;

  @override
  void paint(Canvas canvas, Size size) {
    final n = image.moduleCount + 2 * _quiet;
    final m = size.width / n;
    canvas.drawRect(Offset.zero & size, Paint()..color = light);
    final ink = Paint()..color = dark;
    for (var r = 0; r < image.moduleCount; r++) {
      for (var c = 0; c < image.moduleCount; c++) {
        if (image.isDark(r, c)) {
          canvas.drawRect(
            Rect.fromLTWH((c + _quiet) * m, (r + _quiet) * m, m + 0.3, m + 0.3),
            ink,
          );
        }
      }
    }
  }

  @override
  bool shouldRepaint(_QrPainter old) =>
      old.image != image || old.light != light || old.dark != dark;
}
