"""Draws the Protectorate logo and writes every platform icon.

Run from the repo root when the logo changes: python3 app/tool/icons.py
Needs Pillow. The outputs are committed; builds never run this.
"""

from PIL import Image, ImageDraw

LIT = (0xFF, 0xB5, 0x2E, 255)
GLASS = (0x07, 0x08, 0x0A, 255)
CLEAR = (0, 0, 0, 0)
SS = 4096


def bezier(p0, p1, p2, p3, n=48):
    pts = []
    for i in range(n + 1):
        t = i / n
        a, b, c, d = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t * t, t ** 3
        pts.append((a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
                    a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]))
    return pts


def shield(cx, top, w, h, chamfer):
    l, r, bot, mid = cx - w / 2, cx + w / 2, top + h, top + h * 0.5
    pts = [(l + chamfer, top), (r - chamfer, top), (r, top + chamfer), (r, mid)]
    pts += bezier((r, mid), (r, mid + h * 0.3), (cx + w * 0.18, bot - h * 0.08), (cx, bot))[1:]
    pts += bezier((cx, bot), (cx - w * 0.18, bot - h * 0.08), (l, mid + h * 0.3), (l, mid))[1:]
    return pts + [(l, top + chamfer)]


def shapes(s, scale, bold):
    """The logo as (kind, geometry) on an s-pixel canvas, art scaled about the center."""
    o = s * (1 - scale) / 2
    k = scale
    cx = s / 2
    w, h, top = s * 0.62 * k, s * 0.72 * k, o + s * 0.13 * k
    st = s * (0.095 if bold else 0.072) * k
    lw = s * (0.085 if bold else 0.066) * k
    x0, y0 = cx - w * 0.20, top + h * 0.27
    ux, uy = cx + w * 0.02, y0 + h * 0.26
    return [
        ('fill', shield(cx, top, w, h, w * 0.16)),
        ('cut', shield(cx, top + st, w - 2 * st, h - 2 * st * 1.35, (w - 2 * st) * 0.13)),
        ('stroke', ([(x0, y0), (x0 + w * 0.16, y0 + h * 0.13), (x0, y0 + h * 0.26)], lw)),
        ('bar', ((ux, uy - lw / 2, ux + w * 0.20, uy + lw / 2), lw * 0.25)),
    ]


def render(size, tile='rounded', scale=1.0):
    s = SS
    im = Image.new('RGBA', (s, s), CLEAR)
    d = ImageDraw.Draw(im)
    if tile == 'rounded':
        d.rounded_rectangle([0, 0, s - 1, s - 1], radius=int(s * 0.22), fill=GLASS)
    elif tile == 'full':
        d.rectangle([0, 0, s - 1, s - 1], fill=GLASS)
    for kind, g in shapes(s, scale, bold=size <= 32):
        if kind == 'fill':
            d.polygon(g, fill=LIT)
        elif kind == 'cut':
            d.polygon(g, fill=GLASS)
        elif kind == 'stroke':
            pts, w = g
            for a, b in zip(pts, pts[1:]):
                d.line([a, b], fill=LIT, width=int(w))
            for x, y in pts:
                d.ellipse([x - w / 2, y - w / 2, x + w / 2, y + w / 2], fill=LIT)
        else:
            box, r = g
            d.rounded_rectangle(box, radius=r, fill=LIT)
    return im.resize((size, size), Image.LANCZOS)


def svg(path):
    s = 1024
    hexc = lambda c: '#%02X%02X%02X' % c[:3]
    pts = lambda p: ' '.join('%.1f,%.1f' % q for q in p)
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d">' % (s, s),
           '<rect width="%d" height="%d" rx="%.1f" fill="%s"/>' % (s, s, s * 0.22, hexc(GLASS))]
    for kind, g in shapes(s, 1.0, bold=False):
        if kind in ('fill', 'cut'):
            out.append('<polygon points="%s" fill="%s"/>' % (pts(g), hexc(LIT if kind == 'fill' else GLASS)))
        elif kind == 'stroke':
            out.append('<polyline points="%s" fill="none" stroke="%s" stroke-width="%.1f" '
                       'stroke-linecap="round" stroke-linejoin="round"/>' % (pts(g[0]), hexc(LIT), g[1]))
        else:
            (x0, y0, x1, y1), r = g
            out.append('<rect x="%.1f" y="%.1f" width="%.1f" height="%.1f" rx="%.1f" fill="%s"/>'
                       % (x0, y0, x1 - x0, y1 - y0, r, hexc(LIT)))
    with open(path, 'w') as f:
        f.write('\n'.join(out + ['</svg>']) + '\n')


def main():
    svg('docs/logo.svg')
    render(32).save('app/web/favicon.png')
    for n in (192, 512):
        render(n).save('app/web/icons/Icon-%d.png' % n)
        render(n, tile='full', scale=0.72).save('app/web/icons/Icon-maskable-%d.png' % n)
    for d, n in (('mdpi', 48), ('hdpi', 72), ('xhdpi', 96), ('xxhdpi', 144), ('xxxhdpi', 192)):
        render(n).save('app/android/app/src/main/res/mipmap-%s/ic_launcher.png' % d)
    for n in (16, 32, 64, 128, 256, 512, 1024):
        render(n).save('app/macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_%d.png' % n)
    sizes = (16, 24, 32, 48, 64, 128, 256)
    frames = [render(n) for n in sizes]
    frames[-1].save('app/windows/runner/resources/app_icon.ico', sizes=[(n, n) for n in sizes],
                    append_images=frames[:-1])


if __name__ == '__main__':
    main()
