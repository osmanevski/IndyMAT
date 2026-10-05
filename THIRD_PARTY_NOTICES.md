# Third-party components

The MIT license in [LICENSE](LICENSE) covers the original application code only. GNU Octave, its packages and other third-party components retain their respective licenses, listed below and in the bundled package license files.

- GNU Octave: separately installed runtime, GNU GPL v3 or later. https://octave.org
- control 4.2.3: `.packages/control-4.2.3/packinfo/COPYING` and its SLICOT license notices.
- signal 1.4.8: `.packages/signal-1.4.8/packinfo/COPYING`.
- datatypes 1.5.0: `.packages/datatypes-1.5.0/packinfo/COPYING`.
- CodeMirror 6 and related editor packages: MIT; notices in `licenses/` and source package repositories.
- MathJax 3.2.2: Apache License 2.0; bundled locally for offline publish-time TeX-to-SVG conversion. See `licenses/mathjax-LICENSE`.
- esbuild and Playwright: build/test dependencies only; versioned in package-lock.json with their upstream license files in node_modules when installed.

GNU Octave and package binaries are machine-specific dependencies; preserve their accompanying notices when distributing them. They are not included in this source repository.

The IndyMAT mark and logo (`docs/brand/`, `static/icon.png`, `static/favicon.*`, `macos/icon.png`) are original project artwork; the lettering in the logo files is converted to outlines.

- Montserrat SemiBold (`static/fonts/montserrat-latin-600-normal.woff2`): SIL Open Font License 1.1, bundled locally and used only for the product name. See `licenses/montserrat-OFL.txt`. The brand kit in `docs/brand/` carries further Montserrat weights for documents and the web under the same license (`docs/brand/fonts/OFL.txt`); the application does not load them.
