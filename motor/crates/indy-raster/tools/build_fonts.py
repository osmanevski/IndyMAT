#!/usr/bin/env python3
"""Subset TrueType fonts into the compact outline files of indy-raster.

Usage: python3 -I build_fonts.py <in.ttf> <out.bin> [<in.ttf> <out.bin> ...]

Only the stdlib. Reads head/hhea/maxp/cmap/hmtx/loca/glyf, resolves composite
glyphs and writes, for the code points listed in CHARS:

  b"IRF1", u16 units_per_em, i16 ascent, i16 descent, u16 count,
  count * (u24 code point, u16 advance, u32 outline offset), outline stream.

An outline is: varint contours, then per contour varint points, then per point
varint(zigzag(dx) << 1 | on_curve), varint(zigzag(dy)) (TrueType points, so an
off-curve point is a quadratic control). All integers are big-endian.
"""
import struct
import sys


def ranges(*items):
    out = []
    for item in items:
        if isinstance(item, int):
            out.append(item)
        else:
            out.extend(range(item[0], item[1] + 1))
    return out


CHARS = ranges(
    (0x20, 0x7E), (0xA0, 0xFF), (0x100, 0x17F),  # Latin, Latin-1, Latin Extended-A (Turkish)
    (0x384, 0x3CE), 0x3D1, 0x3D5, 0x3D6,  # Greek
    (0x2010, 0x2015), (0x2018, 0x201E), (0x2020, 0x2022), 0x2026, 0x2030, 0x2032, 0x2033, 0x2039, 0x203A,
    (0x2070, 0x2071), (0x2074, 0x208E), 0x20AC, 0x2122,
    (0x2190, 0x2199), (0x21D0, 0x21D5),
    0x2200, 0x2202, 0x2203, 0x2205, 0x2206, 0x2207, 0x2208, 0x2209, 0x220B, 0x220F, 0x2211, 0x2212, 0x2213,
    0x2215, 0x2217, 0x2218, 0x2219, 0x221A, 0x221D, 0x221E, 0x2220, 0x2227, 0x2228, 0x2229, 0x222A, 0x222B,
    0x2234, 0x223C, 0x2245, 0x2248, 0x2260, 0x2261, 0x2264, 0x2265, 0x2282, 0x2283, 0x2286, 0x2287,
    0x22A5, 0x22C5, 0xFFFD,
)


def u16(b, o): return struct.unpack_from('>H', b, o)[0]
def i16(b, o): return struct.unpack_from('>h', b, o)[0]
def u32(b, o): return struct.unpack_from('>I', b, o)[0]


class Ttf:
    def __init__(self, data):
        self.d = data
        n = u16(data, 4)
        self.t = {data[12 + 16 * i:16 + 16 * i].decode('latin1'): u32(data, 20 + 16 * i) for i in range(n)}
        head = self.t['head']
        self.upem = u16(data, head + 18)
        self.long_loca = i16(data, head + 50) != 0
        hhea = self.t['hhea']
        self.ascent, self.descent = i16(data, hhea + 4), i16(data, hhea + 6)
        self.nhm = u16(data, hhea + 34)
        self.nglyphs = u16(data, self.t['maxp'] + 4)
        self.cmap = self.read_cmap()

    def read_cmap(self):
        d, base = self.d, self.t['cmap']
        best = None
        for i in range(u16(d, base + 2)):
            pid, eid, off = u16(d, base + 4 + 8 * i), u16(d, base + 6 + 8 * i), u32(d, base + 8 + 8 * i)
            fmt = u16(d, base + off)
            if (pid, eid, fmt) in ((3, 10, 12), (0, 4, 12)):
                best = (base + off, 12)
                break
            if (pid, eid, fmt) in ((3, 1, 4), (0, 3, 4)):
                best = (base + off, 4)
        o, fmt = best
        m = {}
        if fmt == 12:
            for g in range(u32(d, o + 12)):
                s, e, gid = struct.unpack_from('>III', d, o + 16 + 12 * g)
                for c in range(s, e + 1):
                    m[c] = gid + c - s
        else:
            seg = u16(d, o + 6) // 2
            ends, starts = o + 14, o + 16 + 2 * seg
            deltas, offs = starts + 2 * seg, starts + 4 * seg
            for i in range(seg):
                e, s, dl, ro = u16(d, ends + 2 * i), u16(d, starts + 2 * i), u16(d, deltas + 2 * i), u16(d, offs + 2 * i)
                for c in range(s, e + 1):
                    if c == 0xFFFF:
                        continue
                    if ro == 0:
                        g = (c + dl) & 0xFFFF
                    else:
                        g = u16(d, offs + 2 * i + ro + 2 * (c - s))
                        g = (g + dl) & 0xFFFF if g else 0
                    if g:
                        m[c] = g
        return m

    def advance(self, g):
        i = min(g, self.nhm - 1)
        return u16(self.d, self.t['hmtx'] + 4 * i)

    def glyph_range(self, g):
        loca = self.t['loca']
        if self.long_loca:
            return u32(self.d, loca + 4 * g), u32(self.d, loca + 4 * g + 4)
        return 2 * u16(self.d, loca + 2 * g), 2 * u16(self.d, loca + 2 * g + 2)

    def contours(self, g, depth=0):
        """[[(x, y, on), ...], ...] with composites resolved."""
        a, b = self.glyph_range(g)
        if a == b or depth > 6:
            return []
        d, o = self.d, self.t['glyf'] + a
        n = i16(d, o)
        o += 10
        if n >= 0:
            ends = [u16(d, o + 2 * i) for i in range(n)]
            o += 2 * n
            o += 2 + u16(d, o)
            count = ends[-1] + 1 if ends else 0
            flags = []
            while len(flags) < count:
                f = d[o]
                o += 1
                flags.append(f)
                if f & 8:
                    flags.extend([f] * d[o])
                    o += 1
            xs, x = [], 0
            for f in flags:
                if f & 2:
                    x += d[o] if f & 16 else -d[o]
                    o += 1
                elif not f & 16:
                    x += i16(d, o)
                    o += 2
                xs.append(x)
            ys, y = [], 0
            for f in flags:
                if f & 4:
                    y += d[o] if f & 32 else -d[o]
                    o += 1
                elif not f & 32:
                    y += i16(d, o)
                    o += 2
                ys.append(y)
            out, s = [], 0
            for e in ends:
                out.append([(xs[i], ys[i], flags[i] & 1) for i in range(s, e + 1)])
                s = e + 1
            return out
        out = []
        while True:
            flags, sub = u16(d, o), u16(d, o + 2)
            o += 4
            if flags & 1:
                dx, dy = i16(d, o), i16(d, o + 2)
                o += 4
            else:
                dx, dy = struct.unpack_from('>bb', d, o)
                o += 2
            xx, xy, yx, yy = 1.0, 0.0, 0.0, 1.0
            if flags & 8:
                xx = yy = i16(d, o) / 16384
                o += 2
            elif flags & 0x40:
                xx, yy = i16(d, o) / 16384, i16(d, o + 2) / 16384
                o += 4
            elif flags & 0x80:
                xx, xy, yx, yy = (i16(d, o + 2 * k) / 16384 for k in range(4))
                o += 8
            for c in self.contours(sub, depth + 1):
                out.append([(round(xx * x + yx * y + dx), round(xy * x + yy * y + dy), on) for x, y, on in c])
            if not flags & 0x20:
                return out


def varint(v, out):
    while v >= 0x80:
        out.append(v & 0x7F | 0x80)
        v >>= 7
    out.append(v)


def zigzag(v):
    return v * 2 if v >= 0 else -v * 2 - 1


def build(src, dst):
    f = Ttf(open(src, 'rb').read())
    index, stream = bytearray(), bytearray()
    kept = [c for c in CHARS if c in f.cmap]
    for c in kept:
        g = f.cmap[c]
        index += struct.pack('>I', c)[1:] + struct.pack('>HI', f.advance(g), len(stream))
        cs = f.contours(g)
        varint(len(cs), stream)
        for contour in cs:
            varint(len(contour), stream)
            px, py = 0, 0
            for x, y, on in contour:
                varint(zigzag(x - px) << 1 | on, stream)
                varint(zigzag(y - py), stream)
                px, py = x, y
    head = b'IRF1' + struct.pack('>HhhH', f.upem, f.ascent, f.descent, len(kept))
    open(dst, 'wb').write(head + bytes(index) + bytes(stream))
    missing = [hex(c) for c in CHARS if c not in f.cmap]
    print(f'{dst}: {len(kept)} glyphs, {len(head) + len(index) + len(stream)} bytes, upem {f.upem}, '
          f'hhea {f.ascent}/{f.descent}; missing: {" ".join(missing)}')


if __name__ == '__main__':
    args = sys.argv[1:]
    for k in range(0, len(args), 2):
        build(args[k], args[k + 1])
