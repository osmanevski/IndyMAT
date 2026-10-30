# Embedded fonts

`indy-raster` draws all text from outlines embedded in the binary; no system
font is read. The `.bin` files are subsets written by `tools/build_fonts.py`
(Latin, Latin-1, Latin Extended-A with Turkish ç ğ ı İ ö ş ü, Greek, common
punctuation, arrows and math symbols). Outlines are unchanged; hinting,
kerning and every other table are dropped.

| File | Derived from | Licence |
|---|---|---|
| `sans-regular.bin` | Liberation Sans 2.1.5 Regular (github.com/liberationfonts) | SIL OFL 1.1, `OFL-LiberationSans.txt` |
| `sans-bold.bin` | Liberation Sans 2.1.5 Bold | SIL OFL 1.1, `OFL-LiberationSans.txt` |
| `mono-regular.bin` | JetBrains Mono 2.304 Regular (github.com/JetBrains/JetBrainsMono) | SIL OFL 1.1, `OFL-JetBrainsMono.txt` |

The OFL allows embedding in software. Because a subset is a modified version
and "Liberation" is a Reserved Font Name, the files here do not carry that
name; this note is the acknowledgement of their origin. They are not sold or
installed as fonts.

Why these: the UI's tick font is JetBrains Mono. For `-apple-system` the
measured Chromium uses a Helvetica-metric face: Liberation Sans reproduced its
width for every sample string (`gorsel/raster/font-olcum.json`, ratio 1.000 at
10, 11, 12, 14 and 20 px, regular and 600). It is also the free counterpart of
MATLAB's default figure font.

Characters missing from the sans fonts come from the mono font (math symbols,
arrows). Known gaps: `∝` (U+221D) and `∠` (U+2220) are in neither font and draw
as an empty box, as does anything outside the subset.

Rebuild:

    python3 -I tools/build_fonts.py LiberationSans-Regular.ttf fonts/sans-regular.bin \
        LiberationSans-Bold.ttf fonts/sans-bold.bin \
        JetBrainsMono-Regular.ttf fonts/mono-regular.bin
