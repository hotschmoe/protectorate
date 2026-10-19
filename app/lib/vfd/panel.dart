import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import 'lamp.dart';

// A piece of hardware: a bezel with its legend printed on the face, around a
// recessed glass window the readings sit in.
class Panel extends StatelessWidget {
  const Panel({
    super.key,
    required this.child,
    this.legend,
    this.trailing,
    this.tone,
    this.padding = const EdgeInsets.all(Tokens.gap),
    this.onTap,
  });

  final Widget child;
  final String? legend;
  final Widget? trailing;
  final Tone? tone;
  final EdgeInsets padding;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final p = t.p;
    final edge = tone == null ? p.bezel : t.tone(tone!);
    final window = ClipRRect(
      borderRadius: BorderRadius.circular(Tokens.bezelRadius - 3),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: p.glass,
          gradient: t.dark
              ? LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [
                    Color.alphaBlend(p.emph.withValues(alpha: 0.025), p.glass),
                    p.glass,
                  ],
                  stops: const [0, 0.35],
                )
              : null,
          border: Border.all(
            color: t.dark ? p.shadow.withValues(alpha: 0.9) : p.bezel,
          ),
          borderRadius: BorderRadius.circular(Tokens.bezelRadius - 3),
        ),
        child: Padding(padding: padding, child: child),
      ),
    );
    final hasHead = legend != null || trailing != null;
    final body = Container(
      decoration: BoxDecoration(
        color: p.frame,
        borderRadius: BorderRadius.circular(Tokens.bezelRadius),
        border: Border.all(
          color: tone == null ? edge : edge.withValues(alpha: 0.8),
          width: tone == null ? 1 : 1.5,
        ),
        boxShadow: t.bezelShadow,
      ),
      child: Stack(
        children: [
          Positioned(
            left: Tokens.bezelRadius,
            right: Tokens.bezelRadius,
            top: 0,
            child: Container(height: 1, color: p.bezelHi),
          ),
          Padding(
            padding: EdgeInsets.fromLTRB(4, hasHead ? 0 : 4, 4, 4),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                if (hasHead)
                  ConstrainedBox(
                    constraints: const BoxConstraints(minHeight: 30),
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(10, 6, 8, 6),
                      child: Row(
                        children: [
                          if (legend != null)
                            Expanded(
                              child: Text(
                                legend!,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: t.legend(
                                  tone == null ? p.legend : t.tone(tone!),
                                ),
                              ),
                            )
                          else
                            const Spacer(),
                          ?trailing,
                        ],
                      ),
                    ),
                  ),
                window,
              ],
            ),
          ),
        ],
      ),
    );
    if (onTap == null) return body;
    return Material(
      type: MaterialType.transparency,
      child: InkWell(
        borderRadius: BorderRadius.circular(Tokens.bezelRadius),
        onTap: onTap,
        child: body,
      ),
    );
  }
}

String ghostDigits(String value) => value.replaceAll(RegExp(r'[0-9-]'), '8');

// Segment digits over their unlit ghosts, each part followed by its unit.
class SegmentReadout extends StatelessWidget {
  SegmentReadout({
    super.key,
    required String value,
    String unit = '',
    this.size = Tokens.readoutL,
    this.color,
  }) : parts = [(value, unit)];

  const SegmentReadout.parts({
    super.key,
    required this.parts,
    this.size = Tokens.readoutL,
    this.color,
  });

  final List<(String, String)> parts;
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final lit = t.readout(size, color);
    final ghost = t.readout(size, t.p.ghost).copyWith(shadows: const []);
    final unitStyle = t
        .legend(t.p.dim)
        .copyWith(fontSize: size >= Tokens.readoutL ? 12 : 11);
    return Semantics(
      label: parts.map((e) => '${e.$1} ${e.$2}'.trim()).join(' '),
      excludeSemantics: true,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          for (var i = 0; i < parts.length; i++) ...[
            if (i > 0) SizedBox(width: size * 0.55),
            Stack(
              children: [
                Text(ghostDigits(parts[i].$1), style: ghost),
                Text(parts[i].$1, style: lit),
              ],
            ),
            if (parts[i].$2.isNotEmpty)
              Padding(
                padding: EdgeInsets.only(left: size * 0.12, bottom: size * 0.1),
                child: Text(parts[i].$2, style: unitStyle),
              ),
          ],
        ],
      ),
    );
  }
}

// A legend over a readout: one number that matters on a board.
class ReadoutTile extends StatelessWidget {
  const ReadoutTile({
    super.key,
    required this.legend,
    required this.readout,
    this.lamp,
    this.footer,
  });

  final String legend;
  final Widget readout;
  final Lamp? lamp;
  final String? footer;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (lamp != null) ...[lamp!, const SizedBox(width: 6)],
            Text(legend, style: t.legend()),
          ],
        ),
        const SizedBox(height: 6),
        readout,
        if (footer != null) ...[
          const SizedBox(height: 4),
          Text(footer!, style: t.small(), maxLines: 1),
        ],
      ],
    );
  }
}

// A table row on the printout: alternate rows carry the paper's bar.
class BarRow extends StatelessWidget {
  const BarRow({
    super.key,
    required this.index,
    required this.child,
    this.padding = const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
  });

  final int index;
  final Widget child;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Container(
      padding: padding,
      decoration: BoxDecoration(
        color: index.isOdd ? t.p.bar : null,
        borderRadius: BorderRadius.circular(Tokens.innerRadius),
      ),
      child: child,
    );
  }
}

// A message line with its lamp: notes, errors, warnings.
class NoteStrip extends StatelessWidget {
  const NoteStrip(this.text, {super.key, this.tone = Tone.warn});

  final String text;
  final Tone tone;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = t.tone(tone);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        color: t.tint(c),
        border: Border.all(color: c.withValues(alpha: 0.6)),
        borderRadius: BorderRadius.circular(Tokens.innerRadius + 2),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 5),
            child: Lamp(tone: tone),
          ),
          const SizedBox(width: 10),
          Expanded(child: Text(text, style: t.data(c))),
        ],
      ),
    );
  }
}

// The display fades up from its unlit state the first time it is painted.
class WarmUp extends StatefulWidget {
  const WarmUp({super.key, required this.child});

  final Widget child;

  @override
  State<WarmUp> createState() => _WarmUpState();
}

class _WarmUpState extends State<WarmUp> with TickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: Tokens.warmUp);

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.disableAnimationsOf(context)) {
      _c.value = 1;
    } else if (!_c.isAnimating && _c.value == 0) {
      _c.forward();
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FadeTransition(
    opacity: CurvedAnimation(parent: _c, curve: Curves.easeOutQuad),
    child: widget.child,
  );
}
