# Envoy WebUI Theming Specification

This document defines the visual themes and UX patterns for the Envoy WebUI.

---

## Decision: Neo-Noir Cyberpunk Theme

**Status**: APPROVED
**Date**: 2026-01-27

After reviewing mockups, we are implementing the **Neo-Noir Cyberpunk** theme as the sole theme for the Envoy WebUI. This decision honors the Altered Carbon source material and provides a distinctive, cohesive visual identity.

### Mockups

Working HTML mockups are available in `docs/mockups/`:

| File | Description |
|------|-------------|
| `01-cyberpunk-dashboard.html` | Sleeve registry dashboard with status cards |
| `03-cyberpunk-needlecast.html` | Needlecast messaging view (3-column layout) |
| `04-cyberpunk-components.html` | Component library reference |

Open these directly in a browser to preview the design.

### Rationale

- Honors the Altered Carbon universe aesthetic (sleeves, cortical stacks, Protectorate)
- Distinctive visual identity that sets Protectorate apart
- Medium density balances information and readability
- Glow effects and color coding provide clear status at a glance

### Not Implementing

- **NORAD Command theme**: Deferred. May revisit for terminal/TUI mode later.
- **Theme switching**: Single theme simplifies implementation and maintenance.

---

## Overview

| Theme | Status | Notes |
|-------|--------|-------|
| Neo-Noir Cyberpunk | **IMPLEMENTING** | Primary and only theme |
| NORAD Command | Deferred | Reference preserved below for future consideration |
| Medical Monitor | Reference only | Influences status visualization patterns |

---

## Theme 1: Neo-Noir Cyberpunk (Primary)

### Philosophy

The Altered Carbon universe treats bodies as "sleeves" - interchangeable vessels for consciousness stored in "cortical stacks." Our UI should feel like a Protectorate operations console: sleek, dangerous, corporate-dystopian. Think Blade Runner meets corporate surveillance tech.

### Color Palette

```
Background Layers:
  --bg-deep:      #0a0a0f    (near-black, main background)
  --bg-surface:   #12121a    (card/panel backgrounds)
  --bg-elevated:  #1a1a24    (hover states, modals)
  --bg-overlay:   #0a0a0fcc  (semi-transparent overlays)

Primary Accents (Cyan family - "stack active"):
  --cyan-glow:    #00f5ff    (primary actions, healthy status)
  --cyan-dim:     #00a5aa    (secondary elements)
  --cyan-subtle:  #004d4f    (borders, inactive)

Warning Accents (Amber family - "attention"):
  --amber-glow:   #ffaa00    (warnings, pending states)
  --amber-dim:    #aa7700    (secondary warnings)

Critical Accents (Magenta family - "critical/error"):
  --magenta-glow: #ff0055    (errors, critical alerts)
  --magenta-dim:  #aa0038    (secondary critical)

Text:
  --text-primary:   #e0e0e0  (main content)
  --text-secondary: #808090  (labels, metadata)
  --text-muted:     #505060  (disabled, timestamps)
```

### Typography

```
Font Stack:
  Primary:   'JetBrains Mono', 'Fira Code', monospace
  Display:   'Orbitron', 'Rajdhani', sans-serif (headers only)

Sizes:
  --text-xs:   0.75rem   (timestamps, metadata)
  --text-sm:   0.875rem  (labels, secondary)
  --text-base: 1rem      (body text)
  --text-lg:   1.25rem   (section headers)
  --text-xl:   1.5rem    (page titles)
  --text-2xl:  2rem      (hero elements)

Weight:
  Labels/metadata: 400
  Body text: 400
  Headers: 600
  Emphasis: 700
```

### Visual Effects

**Glow Effects** (use sparingly):
```css
/* Active/healthy elements */
.glow-cyan {
  box-shadow: 0 0 10px var(--cyan-glow),
              0 0 20px var(--cyan-subtle);
}

/* Critical alerts */
.glow-magenta {
  box-shadow: 0 0 10px var(--magenta-glow),
              0 0 20px var(--magenta-dim);
  animation: pulse-critical 1s ease-in-out infinite;
}
```

**Scanlines** (optional, subtle):
```css
.scanlines::after {
  content: '';
  position: absolute;
  inset: 0;
  background: repeating-linear-gradient(
    0deg,
    transparent,
    transparent 2px,
    rgba(0, 0, 0, 0.1) 2px,
    rgba(0, 0, 0, 0.1) 4px
  );
  pointer-events: none;
}
```

**Holographic Borders**:
```css
.holo-border {
  border: 1px solid var(--cyan-subtle);
  background: linear-gradient(
    135deg,
    var(--bg-surface) 0%,
    var(--bg-elevated) 100%
  );
}
```

### Component Patterns

#### Sleeve Card

```
+--[SLEEVE: alice]----------------------------------+
|                                                   |
|  DHF          Claude Code v1.0.23                |
|  STATUS       ████████████░░░░ ACTIVE            |
|  UPTIME       04:23:17                           |
|                                                   |
|  STACK INTEGRITY                                 |
|  [||||||||||||||||||||||||||||||||||||||] 98.7%  |
|                                                   |
|  MEMORY    2.1 / 4.0 GB    CPU    23%            |
|                                                   |
|  [INSPECT]  [NEEDLECAST]  [RESLEEVE]             |
+--------------------------------------------------+

Card states:
  - Healthy:  Cyan left border, subtle cyan glow
  - Warning:  Amber left border, amber glow
  - Critical: Magenta left border, pulsing magenta glow
  - Offline:  Muted colors, no glow, dashed border
```

#### Status Indicators

Borrow from medical monitors - continuous line graphs for real-time data:

```
RESPONSE TIME (ms)
200 |
150 |          /\
100 |    /\   /  \    /\
 50 |___/  \_/    \__/  \___
    +-------------------------> t

Style: Cyan line (#00f5ff) on dark grid (#1a1a24)
       Threshold line in amber if approaching limits
```

#### Needlecast Visualization

Messages between sleeves shown as directed connections:

```
    [alice]
       |
       | "task complete, results in OUTBOX"
       v
    [bob]
       |
       | "acknowledged, processing"
       v
    [carol]

Visual: Animated particles flowing along connection lines
        Line color indicates message age (bright = recent)
```

#### Navigation

Top bar with Protectorate branding:

```
+------------------------------------------------------------------+
| PROTECTORATE //ENVOY                    [cluster: local] [ADMIN] |
+------------------------------------------------------------------+
| SLEEVES | NEEDLECAST | LOGS | CONFIG                             |
+------------------------------------------------------------------+
```

### Animations

Keep animations functional, not decorative:

| Element | Animation | Duration | Purpose |
|---------|-----------|----------|---------|
| Status change | Color fade | 300ms | Draw attention to changes |
| New sleeve | Fade in + slide up | 400ms | Indicate new element |
| Sleeve removal | Fade out | 200ms | Smooth exit |
| Critical alert | Pulse glow | 1s loop | Demand attention |
| Needlecast | Particle flow | Continuous | Show communication activity |

---

## Theme 2: NORAD Command (DEFERRED)

> **Note**: This theme is not being implemented in the initial release. Preserved here for future reference if a compact/TUI mode is needed.

### Philosophy

Cold War-era command centers needed to display maximum information in minimum space. Operators stared at these screens for hours. Every pixel matters. This theme prioritizes information density and scanability over aesthetics.

Ideal for: Terminal environments, SSH sessions, power users, embedded displays.

### Color Palette

```
Background:
  --bg-terminal:  #0a1a0a    (dark green-black)
  --bg-panel:     #0f2010    (slightly lighter)

Text (phosphor green family):
  --green-bright: #33ff33    (active/important)
  --green-normal: #22cc22    (standard text)
  --green-dim:    #118811    (secondary/labels)

Alerts:
  --amber-alert:  #ffaa00    (warnings)
  --red-alert:    #ff3333    (critical)
```

### Typography

```
Font: 'IBM Plex Mono', 'Consolas', monospace
Size: 14px base, fixed (no scaling)
Line height: 1.4
```

### Layout

Dense, table-driven, minimal whitespace:

```
================================================================================
  PROTECTORATE COMMAND v0.1.0                           2026-01-27 14:23:45 UTC
  CLUSTER: local | SLEEVES: 3/3 NOMINAL | DEFCON: 5
================================================================================

  ID       NAME     DHF           STATUS    CPU   MEM     UPTIME    LAST MSG
  -------- -------- ------------- --------- ----- ------- --------- ----------
  a1b2c3d4 alice    claude-code   ACTIVE    23%   2.1GB   04:23:17  00:00:45
  e5f6g7h8 bob      gemini-cli    IDLE       2%   512MB   02:11:45  00:05:12
  i9j0k1l2 carol    opencode      ALERT!    89%   3.8GB   00:05:33  00:00:03

================================================================================
  RECENT NEEDLECAST                                              [F1: HELP]
--------------------------------------------------------------------------------
  14:23:42 alice -> bob      "handoff task #47, see OUTBOX for context"
  14:23:45 bob   -> alice    "ACK, processing"
  14:23:48 carol -> GLOBAL   "WARNING: memory pressure, may need resleeve"
================================================================================
  CMD> _
```

### DEFCON Levels (Status Summary)

Borrow the DEFCON concept for cluster health:

| Level | Meaning | Visual |
|-------|---------|--------|
| DEFCON 5 | All systems nominal | Green text |
| DEFCON 4 | Minor issues, monitoring | Green text, amber indicator |
| DEFCON 3 | Significant issues, attention needed | Amber text |
| DEFCON 2 | Critical issues, intervention likely | Amber text, flashing |
| DEFCON 1 | System failure, immediate action | Red text, alarm |

### Keyboard Navigation

TUI mode should be fully keyboard-navigable:

```
Keybindings:
  j/k or arrows  Navigate sleeve list
  Enter          Inspect selected sleeve
  n              Open needlecast panel
  l              View logs
  r              Trigger resleeve
  /              Filter/search
  q              Back/close panel
  ?              Help
```

### Alert Visualization

Critical alerts use ASCII box drawing and inverse video:

```
+==============================================================================+
|  *** ALERT ***  SLEEVE carol MEMORY CRITICAL (95%)  *** ALERT ***            |
|  Recommended action: RESLEEVE or increase memory limit                       |
|  [A]cknowledge  [R]esleeve  [I]gnore                                         |
+==============================================================================+
```

---

## Theme 3: Medical Monitor (Inspiration Reference)

This theme is not implemented directly but serves as a reference for status visualization patterns that should be incorporated into the other themes.

### Key Concepts to Borrow

#### 1. Vital Signs Layout

Medical monitors group related metrics and show trends:

```
+--[ VITALS ]--------------------+
|                                |
|  HEART RATE        72 bpm     |
|  ~~~\/\/\/\/\/\/~~~           |
|                                |
|  BLOOD PRESSURE   120/80      |
|  SpO2             98%         |
|  TEMP             98.6F       |
+--------------------------------+
```

**Apply to sleeves as**:
```
+--[ SLEEVE VITALS ]-------------+
|                                |
|  RESPONSE TIME     142ms      |
|  ~~~\/\/\/\/\/\/~~~           |
|                                |
|  MEMORY           2.1/4.0 GB  |
|  CPU              23%         |
|  STACK INTEGRITY  98.7%       |
+--------------------------------+
```

#### 2. Waveform Displays

Real-time line graphs for continuous metrics:

- Response time over last 5 minutes
- Memory usage trend
- Message throughput

Style: Single-color line on dark background, grid lines for scale.

#### 3. Threshold Alerts

Medical monitors show when values approach dangerous ranges:

```
Normal:    ||||||||||||||||____________  (green fill)
Warning:   ||||||||||||||||||||||______  (amber fill, threshold line)
Critical:  ||||||||||||||||||||||||||||  (red fill, flashing)
```

#### 4. Triage Colors

Standard medical triage uses consistent colors:

| Color | Medical Meaning | Protectorate Meaning |
|-------|-----------------|----------------------|
| Green | Minor/stable | Healthy, no action needed |
| Yellow | Delayed/observation | Warning, monitor closely |
| Red | Immediate/critical | Critical, intervention needed |
| Black | Deceased/expectant | Offline, unreachable |

#### 5. Alarm Fatigue Prevention

Medical UIs are designed to avoid alarm fatigue:

- Tiered alert severity (info/warn/critical)
- Escalating visual intensity (subtle -> obvious -> impossible to ignore)
- Acknowledgment required to silence persistent alerts
- Clear visual distinction between "new alert" and "acknowledged alert"

---

## Implementation Notes

### CSS Custom Properties

Use CSS custom properties for consistent theming:

```css
:root {
  /* Background Layers */
  --bg-deep: #0a0a0f;
  --bg-surface: #12121a;
  --bg-elevated: #1a1a24;

  /* Cyan Accents */
  --cyan-glow: #00f5ff;
  --cyan-dim: #00a5aa;
  --cyan-subtle: #004d4f;

  /* Amber Accents */
  --amber-glow: #ffaa00;
  --amber-dim: #aa7700;

  /* Magenta Accents */
  --magenta-glow: #ff0055;
  --magenta-dim: #aa0038;

  /* Text */
  --text-primary: #e0e0e0;
  --text-secondary: #808090;
  --text-muted: #505060;
}
```

### Responsive Behavior

| Breakpoint | Behavior |
|------------|----------|
| Desktop (1200px+) | Full layout, all effects, grid cards |
| Tablet (768-1199px) | Reduced glow effects, card stack |
| Mobile (<768px) | Single column cards, simplified layout |

### Accessibility

Both themes must maintain WCAG 2.1 AA compliance:

- Minimum 4.5:1 contrast ratio for text
- Don't rely solely on color for status (use icons/text labels)
- Respect `prefers-reduced-motion` (disable animations)
- Support `prefers-color-scheme` (suggest appropriate theme)

---

## Asset Requirements

### Fonts (Google Fonts CDN)

- **JetBrains Mono** - Primary monospace font for all text
- **Orbitron** - Display font for headers and branding

### Icons

Minimal icon set, monochrome, designed for small sizes:

- Status: healthy, warning, critical, offline
- Actions: inspect, resleeve, send message, terminate
- Navigation: sleeves, messages, logs, settings

Recommend: Lucide Icons or custom SVG set.

---

## Future Considerations

### NORAD Compact Mode

If demand exists for a high-density terminal view, revisit the NORAD theme specification above.

### Holographic Visualization (WebGL)

A more advanced 3D visualization could render sleeves as:
- Floating cortical stack models
- Particle-based needlecast streams
- Spatial arrangement by relationship/communication frequency

Deferred until core WebUI is stable.

### Audio Alerts (Optional)

- Soft notification on new needlecast
- Alert sound on critical status (user-configurable, default OFF)

---

## Reference Material

Visual inspiration sources:

- Altered Carbon (Netflix) - UI screens, Protectorate aesthetics
- Blade Runner 2049 - Environmental UI, holographics
- WarGames (1983) - WOPR interface, NORAD command center
- Alien (1979) - Mother interface, CRT aesthetics
- Medical: Philips IntelliVue, GE CARESCAPE monitors
