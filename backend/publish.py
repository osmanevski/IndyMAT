"""One strict, stdlib-only static-report boundary for Octave and MathJax.

Accept a small balanced HTML/SVG language, not arbitrary HTML5. Rebuilding the
document avoids browser error-recovery/namespace tricks and pins its first head
child to our CSP. Unsupported raw publish HTML fails visibly, never silently.
"""
from backend.i18n import tr
import base64
from html import escape
from html.parser import HTMLParser
import re
from urllib.parse import unquote

MAX_BYTES = 20_000_000
MAX_TAGS = 50_000
MAX_DEPTH = 128
MAX_ATTRIBUTE = 262_144
MAX_IMAGE_ATTRIBUTE = 16_000_000
MAX_ATTRIBUTES = 48
DISK_CSP = "default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'"
VIEW_CSP = "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'"
VIEW_HEADER_CSP = VIEW_CSP + "; frame-ancestors 'none'; sandbox"
HTML_TAGS = set(('html head title meta style body h1 h2 h3 h4 h5 h6 p br hr '
                 'ul ol li a pre code span b i em strong tt sub sup table thead '
                 'tbody tfoot tr td th caption colgroup col img footer div mjx-container').split())
SVG_TAGS = set('svg g path defs use rect line text title'.split())
VOID = {'meta', 'br', 'hr', 'img', 'col'}
GLOBAL_ATTRS = set('id class title lang dir role aria-label aria-labelledby aria-hidden style'.split())
ATTRS = {
    'meta': {'charset', 'http-equiv', 'content'},
    'a': {'href', 'name'}, 'img': {'src', 'alt', 'width', 'height'},
    'td': {'colspan', 'rowspan'}, 'th': {'colspan', 'rowspan', 'scope'},
    'col': {'span'}, 'ol': {'start', 'type', 'reversed'}, 'li': {'value'},
    'style': {'type', 'data-mf-publish-math'},
    'mjx-container': {'jax', 'display', 'data-tex-source'},
}
SVG_ATTRS = set(('xmlns xmlns:xlink width height viewbox preserveaspectratio '
                 'focusable x y x1 y1 x2 y2 dx dy d transform fill stroke '
                 'stroke-width stroke-linecap stroke-linejoin stroke-dasharray '
                 'stroke-dashoffset fill-rule opacity fill-opacity stroke-opacity '
                 'font-family font-size font-weight font-style text-anchor '
                 'data-c data-mml-node data-mjx-texclass data-mjx-variant '
                 'data-frame data-line data-table data-labels data-variant data-background').split())
SVG_CASE = {'viewbox': 'viewBox', 'preserveaspectratio': 'preserveAspectRatio'}
CSS_PROPERTIES = set(('color background background-color font-family font-size '
                      'font-weight font-style line-height text-align text-decoration '
                      'white-space vertical-align display direction position '
                      'margin margin-top margin-bottom margin-left margin-right '
                      'padding padding-top padding-bottom padding-left padding-right '
                      'width height min-width min-height max-width max-height '
                      'border border-width border-style border-color border-collapse '
                      'border-spacing overflow overflow-x overflow-y float clear '
                      'stroke stroke-width stroke-linecap stroke-linejoin '
                      'stroke-dasharray fill opacity').split())
RASTER_TYPES = {'png': 'image/png', 'jpg': 'image/jpeg', 'jpeg': 'image/jpeg',
                'gif': 'image/gif', 'webp': 'image/webp', 'avif': 'image/avif'}


def _fail(detail):
    raise ValueError(tr('The published report is unsafe: {detail}', detail=detail))


def _css(css, stylesheet=False):
    # No escapes, at-rules, custom properties, resource functions or general
    # CSS functions. None is needed by the shipped Octave/MathJax styles.
    # Scan comments/rules linearly; an unclosed comment or a long string with
    # no opening brace must not cause regex backtracking/quadratic work.
    chunks=[];pos=0
    while True:
        start=css.find('/*',pos)
        if start<0:chunks.append(css[pos:]);break
        end=css.find('*/',start+2)
        if end<0:_fail(tr('malformed CSS comment.'))
        chunks.append(css[pos:start]);pos=end+2
    clean=''.join(chunks)
    if any(c in clean for c in ('\\', '@', '<', '\x00')) or '/*' in clean or '*/' in clean:
        _fail(tr('unsupported CSS.'))
    declarations = [clean]
    if stylesheet:
        declarations = [];pos = 0
        while pos<len(clean):
            opening=clean.find('{',pos)
            if opening<0 and not clean[pos:].strip():break
            closing=clean.find('}',opening+1) if opening>=0 else -1
            if opening<0 or closing<0:_fail(tr('malformed stylesheet.'))
            selector=clean[pos:opening].strip();block=clean[opening+1:closing]
            if '{' in block or not re.fullmatch(r'[\w\s.#*,:>+~\[\]="\'|^-]+', selector, re.ASCII):
                _fail(tr('unsupported CSS selector.'))
            declarations.append(block);pos=closing+1
    for block in declarations:
        for declaration in block.split(';'):
            if not declaration.strip():continue
            name, colon, value = declaration.partition(':')
            if not colon or name.strip().lower() not in CSS_PROPERTIES:
                _fail(tr('unsupported CSS property.'))
            if not re.fullmatch(r'[\w\s#.,%+*/()!\-"\']*', value, re.ASCII):
                _fail(tr('unsupported CSS value.'))
            if ('(' in value or ')' in value) and not re.fullmatch(
                    r'\s*(?:rgb|rgba|hsl|hsla)\([\d\s.,%+\-]*\)\s*(?:!important)?\s*', value, re.I):
                _fail(tr('CSS resources and functions are not supported.'))


def _navigation(value):
    normalized = re.sub(r'[\x00-\x20\x7f]', '', unquote(value)).replace('\\', '/')
    if re.search(r'(?:mathjax[^/]*|tex-svg)\.js(?:[?#]|$)', normalized, re.I):
        _fail(tr('References to the MathJax script cannot remain.'))
    scheme = re.match(r'^([a-z][a-z0-9+.-]*):', normalized, re.I)
    if scheme and scheme[1].lower() not in {'http', 'https'}:
        _fail(tr('links must use HTTP(S), a relative path, or a document anchor.'))


def image_source(value):
    """Return a local relative raster path, or None for a validated data image."""
    if value.startswith('data:'):
        match = re.fullmatch(r'data:(image/(?:png|jpeg|gif|webp|avif));base64,([A-Za-z0-9+/=\s]+)', value)
        if not match:_fail(tr('only static raster image data is allowed.'))
        try:base64.b64decode(re.sub(r'\s', '', match[2]), validate=True)
        except ValueError:_fail(tr('malformed image data.'))
        return None
    decoded = unquote(value)
    if (not decoded or decoded.startswith(('/', '.')) or
            any(c in decoded for c in ':\\?#%\x00\r\n') or
            any(part in ('', '.', '..') for part in decoded.split('/')) or
            decoded.rsplit('.', 1)[-1].lower() not in RASTER_TYPES):
        _fail(tr('an image source must be a relative raster file in the report folder.'))
    return decoded


class _ReportParser(HTMLParser):
    def __init__(self, policy, image_loader):
        super().__init__(convert_charrefs=True)
        self.policy, self.image_loader = policy, image_loader
        self.parts = ['<!doctype html>'];self.stack = [];self.tags = 0
        self.output_bytes=len(self.parts[0])
        self.seen = set();self.has_svg = False;self.images = set()

    def emit(self,text):
        self.output_bytes+=len(text.encode('utf-8'))
        if self.output_bytes>MAX_BYTES:_fail(tr('the 20 MB output limit was exceeded.'))
        self.parts.append(text)

    def handle_decl(self, decl):
        if decl.lower() != 'doctype html':_fail(tr('unsupported document type.'))

    def unknown_decl(self, data):_fail(tr('unsupported declaration.'))
    def handle_pi(self, data):_fail(tr('processing instructions are not supported.'))
    # Omit comments (including Octave's duplicate source-code comment). Keep
    # real code/output text intact. A commented head cannot divert the CSP.

    def handle_starttag(self, tag, attrs):
        self.tags += 1
        if self.tags > MAX_TAGS or len(self.stack) >= MAX_DEPTH:_fail(tr('the tag or depth limit was exceeded.'))
        if len(attrs) > MAX_ATTRIBUTES:_fail(tr('too many attributes.'))
        svg = 'svg' in self.stack or tag == 'svg'
        if tag not in (SVG_TAGS if svg else HTML_TAGS):_fail(tr('unsupported tag: {tag}', tag=tag))
        if tag in ('html', 'head', 'body'):
            expected = {'html': [], 'head': ['html'], 'body': ['html']}[tag]
            if self.stack != expected or tag in self.seen:_fail(tr('malformed document structure.'))
            if tag == 'body' and 'head' not in self.seen:_fail(tr('the report head is missing.'))
            self.seen.add(tag)
        elif not self.stack or self.stack[-1] == 'html':_fail(tr('malformed document structure.'))
        if 'head' in self.stack and tag not in {'title', 'meta', 'style'}:_fail(tr('unsupported content in the head.'))
        if tag in {'style', 'meta'} and self.stack != ['html', 'head']:_fail(tr('style and meta elements are allowed only in the head.'))
        if any(parent in {'style', 'title', 'text', 'path', 'use', 'line', 'rect'} for parent in self.stack):
            _fail(tr('text or SVG leaf elements cannot contain tags.'))
        if tag == 'svg':self.has_svg = True
        allowed = GLOBAL_ATTRS | (SVG_ATTRS if svg else ATTRS.get(tag, set()))
        if tag == 'use':allowed = allowed | {'href', 'xlink:href'}
        output = [];seen_attrs = set();values = dict(attrs)
        for name, value in attrs:
            value = value or ''
            limit=MAX_IMAGE_ATTRIBUTE if tag=='img' and name=='src' else MAX_ATTRIBUTE
            if name in seen_attrs or len(value) > limit:_fail(tr('duplicate or excessively long attribute.'))
            seen_attrs.add(name)
            if name not in allowed or name.startswith('on'):_fail(tr('unsupported attribute: {name}', name=name))
            if name == 'style':_css(value)
            elif name == 'href' and tag == 'a':_navigation(value)
            elif name in {'href', 'xlink:href'}:
                if not re.fullmatch(r'#[A-Za-z0-9_.:-]+', value):_fail(tr('SVG references must stay within the same document.'))
            elif name == 'src':
                local = image_source(value)
                if local:
                    self.images.add(local)
                    if self.image_loader:value = self.image_loader(local)
                    if len(value)>MAX_IMAGE_ATTRIBUTE:_fail(tr('the embedded image is too large.'))
            elif name in ('xmlns', 'xmlns:xlink'):
                if value != {'xmlns': 'http://www.w3.org/2000/svg', 'xmlns:xlink': 'http://www.w3.org/1999/xlink'}[name]:
                    _fail(tr('invalid SVG namespace.'))
            elif svg and name in SVG_ATTRS and name!='d' and not name.startswith('data-'):
                if re.search(r'url\s*\(|[:\\@<>&]', value, re.I):_fail(tr('SVG resource values are not supported.'))
            output.append(' ' + SVG_CASE.get(name, name) + '="' + escape(value, quote=True) + '"')
        if tag == 'meta':
            if (values.get('http-equiv') or '').lower() == 'content-security-policy':return
            if set(values) != {'charset'} or (values['charset'] or '').lower() not in {'utf-8', 'utf8'}:
                _fail(tr('only UTF-8 metadata is supported.'))
        self.emit('<' + tag + ''.join(output) + '>')
        if tag == 'head':
            self.emit('<meta http-equiv="Content-Security-Policy" content="' + escape(self.policy, quote=True) + '">')
        if tag not in VOID:self.stack.append(tag)

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        if tag not in VOID:self.handle_endtag(tag)

    def handle_endtag(self, tag):
        if not self.stack or self.stack[-1] != tag:_fail(tr('unbalanced tag: {tag}', tag=tag))
        self.stack.pop();self.emit('</' + tag + '>')

    def handle_data(self, data):
        if self.stack and self.stack[-1] == 'style':
            _css(data, stylesheet=True);self.emit(data)
        else:
            if (not self.stack or self.stack[-1] in {'html', 'head'}) and data.strip():_fail(tr('text outside the document.'))
            self.emit(escape(data, quote=False))


def sanitize_published_html(html, *, require_svg=False, viewer=False, image_loader=None):
    """Return (rebuilt HTML, local images), or reject with a localized error.

    Only comments, doctype spelling and supplied CSP metadata are discarded.
    This is also the mandatory boundary for reports with no math/rendering.
    """
    if not isinstance(html, str) or len(html.encode('utf-8')) > MAX_BYTES:_fail(tr('the 20 MB limit was exceeded.'))
    if '\x00' in html:_fail(tr('contains a NUL character.'))
    parser = _ReportParser(VIEW_CSP if viewer else DISK_CSP, image_loader)
    parser.feed(html);parser.close()
    if parser.stack or parser.seen != {'html', 'head', 'body'}:_fail(tr('a complete and balanced HTML document is required.'))
    if require_svg and not parser.has_svg:_fail(tr('no rendered equation was found.'))
    result = ''.join(parser.parts)
    if len(result.encode('utf-8')) > MAX_BYTES:_fail(tr('the 20 MB output limit was exceeded.'))
    return result, parser.images


def validate_published_html(html):
    """Validate AND return rebuilt math write-back; callers must save this value."""
    return sanitize_published_html(html, require_svg=True)[0]
