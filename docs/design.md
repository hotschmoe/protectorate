# Client design

## Concept

The client is the front panel of a machine room. The hub, the
subscriptions and the agents working in sleeves are instruments, and each
screen is a panel of them. The display technology is the vacuum fluorescent
display: warm amber filaments behind smoked glass, with the extra phosphors
real multi-colour VFDs carried (red-orange, Noritake blue, filament white).
The designer is an IBM engineer in 1970 with taste: strict grid, small caps
legends, clear hierarchy, every mark a reading or a control.

It is a graphical instrument, not a console. Panels are pieces of hardware: a
graphite chassis, bezels with a lit top edge and a soft shadow, recessed glass
windows the readings glow inside. Meters, gauges, dot-matrix graphs and lamps
are painted, not drawn with characters. Desktop layouts are multi-pane
(sleeve list, chat and status side by side); the phone layout is one column of
the same panels with touch-sized controls.

Light mode is the same panel printed by the line printer: warm continuous-form
paper with buff bars on table rows, black ribbon for normal readings, red
ribbon for alarms, and the same components and grid. Green-bar paper was
dropped with the green phosphor; buff-bar stock existed and suits the palette.

## Palette

Every colour is a token in `app/lib/theme/tokens.dart`; widgets never use a
literal colour.

| Token | Dark (VFD) | Light (printout) | Meaning |
|-------|------------|------------------|---------|
| chassis | `#0E1013` | `#EFE9DC` | page behind panels |
| glass | `#07080A` | `#FBF9F3` | panel window |
| frame | `#181B20` | `#F4EBD7` | bezel face the legends sit on |
| bezel / bezelHi | `#23272D` / `#353B43` | `#D6CDB9` / `#FFFFFF` | panel frame, lit top edge |
| rule | `#2B2417` | `#DED4C0` | rules inside a window |
| bar | `#0C0D10` | `#F4EBD7` | alternate table rows |
| legend | `#8E97A1` | `#6E6352` | silkscreened labels on the bezel |
| lit | `#FFB52E` amber | `#2A2118` ink | primary reading, normal segments |
| dim | `#C08A2E` | `#6B5D4A` | secondary text |
| ghost | `#2A1F0E` | `#E7DECB` | unlit segment, off lamp |
| emph | `#E8F4FF` filament | `#17110A` | headline readouts, user text, code |
| info | `#5CC8FF` blue | `#1D5FA6` | informational, working, selection |
| warn | `#FFE45C` pale yellow | `#965600` ochre | approaching a limit, needs you |
| alarm | `#FF5A1F` red-orange | `#B8321A` red | failed, unreachable, critical |
| ok | `#7CC793` | `#2A6E42` | healthy lamp, diff addition |

Rules of use: amber is the voice of the display and carries most text.
Warning is pale yellow, never amber, so it separates from normal readings;
alarm is red-orange. Blue is information that is not a measurement (working,
selected, links). Green is a sparing semantic accent only: an OK/LIVE lamp
and diff additions; it never fills a meter, background or body text. Diff
deletions are red-orange. On paper normal readings print in ink and only
exceptions take colour, as with a black and red ribbon. Text contrast is at
least 4.5:1 on its background in both themes.

Glow: lit elements in dark mode get a soft shadow of their own colour (text
shadow blur 6 at 45 % for 13 px and up, a blurred underlay for segments and
lamps). No glow below 13 px, none in light mode. Gradients only for depth
(the glass window's faint top-lit falloff), never as colour decoration.

## Type

JetBrains Mono everywhere. DSEG7 Classic (OFL, `assets/fonts/DSEG-OFL.txt`)
only for large numerals: percentages, token counts, countdowns, the clock.
Segment readouts draw ghost `8`s behind the lit digits and put units in small
caps beside them.

| Style | Size | Use |
|-------|------|-----|
| readoutXL / L / M | 40 / 28 / 20 DSEG7 | headline numbers |
| title | 16 w700 | screen and panel titles |
| body | 14, height 1.5 | prose, chat |
| data | 13 | values next to meters, rows |
| small | 12.5 | secondary lines |
| legend | 11 w700, uppercase, tracking 1.4 | small caps legends, never glows |

## Components (`app/lib/vfd/`)

- **Panel**: bezel plus glass window; legend silkscreened on the bezel above
  the window, optional trailing legend. Groups one instrument.
- **SegmentMeter / MeterRow**: discrete segments (default 20, 5 % each); unlit
  segments ghosted, warn and alarm zones tinted so thresholds are visible
  before they are reached. Lit segments take their zone colour. An unknown
  value draws a row of centre dashes and no ghosts, so it can never read as
  spare capacity. MeterRow is legend, meter and a value column that never
  truncates.
- **ArcGauge**: a segmented 270 degree arc with a segment readout in the
  middle, for one headline value per pane (chat context).
- **SegmentReadout**: DSEG7 digits over ghost 8s, unit legend.
- **Lamp / LampLegend**: lit, dim, or blinking (attention). Blink is reserved
  for "needs you" (pending approval).
- **Annunciator**: a strip of legend tiles that light in their state colour:
  hub status, approval prompts.
- **ChoiceKeys**: a row of annunciator keys of which one is lit, a selector
  switch (machine type, drain options).
- **BusyBar**: a segment chaser in place of indeterminate progress.

## Layout

- 4 px grid; gaps 8 / 12 / 16 / 24; bezel radius 10, inner radius 3.
- Phone first (412): one column, 12 px margins, 48 px touch targets.
- Wide (600+): navigation rail; 1200+: multi-pane (overview status plus quota
  and events, sleeve list plus chat plus status).
- Numbers right aligned with tabular figures; legends left; values in a
  column per panel.
- Identifiers (hub, sleeve, project names) keep their case; legends and
  states are uppercase.

## Motion

Segment fill animates on change (260 ms ease-out) and fills from zero on first
paint, the warm-up; the app fades up from the ghost state over 450 ms at
launch; attention lamps blink with a 1.6 s period; the busy bar chases. With
`MediaQuery.disableAnimations` every value jumps and an attention lamp shows a
steady ring instead of blinking.
