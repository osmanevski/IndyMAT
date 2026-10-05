import sys
import unittest
import base64
import io
import tempfile
import threading
import json
import os
import subprocess
import shutil
import queue
import time
from unittest.mock import patch
from email.message import Message
from html.parser import HTMLParser
from pathlib import Path
from types import SimpleNamespace
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from backend.publish import validate_published_html, sanitize_published_html, DISK_CSP, MAX_DEPTH, MAX_TAGS, MAX_ATTRIBUTE
from backend.files import Workspace
from app import Handler, App, PublishKernel


class ReportCspTests(unittest.TestCase):
    @staticmethod
    def request(app, path, data=None):
        # Exercise the real GET and header paths without binding a socket,
        # constructing App/Kernel, or accessing a running application's token.
        handler = Handler.__new__(Handler)
        handler.server = SimpleNamespace(app=app)
        handler.command = 'GET' if data is None else 'POST'
        handler.path = path
        handler.requestline = 'GET ' + path + ' HTTP/1.1'
        handler.request_version = 'HTTP/1.1'
        handler.headers = Message()
        handler.headers['Host'] = f'127.0.0.1:{app.port}'
        handler.headers['X-MF-Token'] = app.token
        if data is not None:
            raw=json.dumps(data).encode();handler.rfile=io.BytesIO(raw)
            handler.headers['Content-Length']=str(len(raw))
        handler.wfile = io.BytesIO()
        handler.handle_request(data is not None)
        header, body = handler.wfile.getvalue().split(b'\r\n\r\n', 1)
        lines = header.decode().split('\r\n')
        return int(lines[0].split()[1]), dict(line.split(': ', 1) for line in lines[1:]), body

    @staticmethod
    def directives(csp):
        return {parts[0]:parts[1:] for item in csp.split(';') if (parts := item.split())}

    def test_page_header_allows_only_blob_frames(self):
        status, headers, _ = self.request(SimpleNamespace(port=12345, token='test-only'), '/')
        self.assertEqual(status, 200)
        policy = self.directives(headers['Content-Security-Policy'])
        self.assertEqual(policy['frame-src'], ['blob:'])
        self.assertEqual(policy['script-src'], ["'self'"])
        self.assertEqual(policy['frame-ancestors'], ["'none'"])
        # Explicit frame-src takes precedence over child-src/default-src.
        self.assertNotIn('child-src', policy)

    def test_report_blob_content_keeps_csp_svg_styles_and_inlines_images(self):
        with tempfile.TemporaryDirectory(prefix='indymat-report-csp-') as directory:
            root = Path(directory).resolve()
            folder = root/'html';folder.mkdir()
            png = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=')
            (folder/'plot.png').write_bytes(png)
            svg = '<svg style="vertical-align: -0.025ex" aria-label="$x^2$"><title>$x^2$</title><defs><path id="glyph" d="M0 0"/></defs><use href="#glyph"/></svg>'
            styles = '<style data-mf-publish-math="">mjx-container > svg { overflow: visible; }</style>'
            heading = '<h2><a id="node1">Equation <mjx-container style="display:inline-block">' + svg + '</mjx-container></a></h2>'
            source = '<!doctype html><html><head>' + styles + '</head><body>' + heading + '<img src="plot.png"></body></html>'
            report = folder/'report.html';report.write_text(source, encoding='utf-8')
            app = SimpleNamespace(port=12345, token='test-only', file_lock=threading.Lock(), workspace=Workspace(root, root))
            status, headers, body = self.request(app, '/api/published?path=' + quote(str(report)))
            self.assertEqual(status, 200)
            self.assertEqual(headers['Content-Type'], 'text/html; charset=utf-8')
            policy = self.directives(headers['Content-Security-Policy'])
            self.assertEqual(policy, {'default-src':["'none'"], 'img-src':['data:'], 'style-src':["'unsafe-inline'"], 'font-src':['data:'], 'frame-ancestors':["'none'"], 'base-uri':["'none'"], 'form-action':["'none'"], 'sandbox':[]})
            html = body.decode('utf-8')
            class MetaParser(HTMLParser):
                content = None
                def handle_starttag(self, tag, attrs):
                    attrs = dict(attrs)
                    if tag == 'meta' and attrs.get('http-equiv') == 'Content-Security-Policy':self.content = attrs.get('content')
            parser = MetaParser();parser.feed(html)
            # Fetch -> Blob preserves these bytes, not the HTTP response CSP.
            # The meta policy travels with the document; iframe sandbox adds
            # the script/origin restriction that cannot be expressed in meta.
            self.assertEqual(self.directives(parser.content), {k:v for k,v in policy.items() if k not in ('frame-ancestors', 'sandbox')})
            self.assertTrue(html.startswith('<!doctype html><html><head><meta '))
            self.assertIn(styles, html)
            self.assertIn(heading.replace('/>', '></path>',1).replace('<use href="#glyph"/>','<use href="#glyph"></use>'), html)
            self.assertIn('src="data:image/png;base64,' + base64.b64encode(png).decode() + '"', html)
            self.assertNotIn('src="plot.png"', html)
            self.assertEqual(report.read_text(encoding='utf-8'), source)


class PublishValidationTests(unittest.TestCase):
    SVG = '<svg role="img" aria-label="$x^2$"><title>$x^2$</title><defs><path id="glyph" d="M0 0"/></defs><use href="#glyph"/></svg>'

    def accept(self, markup):
        validate_published_html('<!doctype html><html><head></head><body>' + self.SVG + markup + '</body></html>')

    def reject(self, markup):
        with self.assertRaises(ValueError):self.accept(markup)

    def test_navigation_and_static_equations(self):
        self.accept('<footer><a href="https://www.octave.org">Octave</a></footer><a href="#node1">Contents</a>')
        self.accept('<a title="a > b" href="http://example.org/?a=1&amp;b=2">$price$</a>')
        self.accept('<a href=//example.org>Link</a><pre>&lt;script&gt; $x$ https://example.org</pre>')
        self.accept('<img src="plot.png">')
        self.accept('<img src="data:image/png;base64,AAAA">')
        self.accept('<svg><use xlink:href="#glyph"/></svg>')

    def test_resource_attributes(self):
        positions = [('link','href'), ('img','src'), ('source','src'), ('iframe','src'),
                     ('embed','src'), ('object','data'), ('object','codebase'), ('object','archive'),
                     ('video','src'), ('video','poster'), ('audio','src'), ('track','src'),
                     ('form','action'), ('input','src'), ('input','formaction'), ('button','formaction'),
                     ('image','href'), ('image','xlink:href'), ('use','href'), ('use','xlink:href')]
        for tag, attr in positions:
            for url in ('https://remote.example/r', '//remote.example/r'):
                with self.subTest(tag=tag, attr=attr, url=url):
                    self.reject(f'<{tag} title="quoted > boundary" {attr}="{url}">')

    def test_attribute_parsing(self):
        for html in ('<IMG title=">" SRC=HTTPS://remote.example/r>',
                     '<img title=\'a > b\' src=\'//remote.example/r\'>',
                     '<img src="local.png" src="https://remote.example/r">',
                     '<img src="&#104;ttps&#58;//remote.example/r">',
                     '<img src="https:\\remote.example/r">',
                     '<img srcset="local.png 1x, https://remote.example/r 2x">',
                     '<source srcset="local.png 1x, //remote.example/r 2x">'):
            with self.subTest(html=html):self.reject(html)

    def test_svg_references_stay_in_document(self):
        for target in ('other.svg#glyph', '/other.svg#glyph', 'data:image/svg+xml,svg'):
            with self.subTest(target=target):self.reject(f'<svg><use href="{target}"/></svg>')

    def test_css_resources(self):
        declarations = ['background: url(https://remote.example/a)',
                        'background: URL("//remote.example/a>b")',
                        "@import 'https://remote.example/a.css';",
                        '@import url(//remote.example/a.css);',
                        'background:url(/*comment*/https://remote.example/a)',
                        r'background:u\72l(\68 ttps://remote.example/a)',
                        r'@\69mport "https://remote.example/a.css";',
                        'background:url("ht\\\ntps://remote.example/a")',
                        'background: url(javascript:alert(1))']
        for css in declarations:
            with self.subTest(css=css):
                self.reject('<style>' + css + '</style>')
                self.reject('<p style="' + css.replace('"', '&quot;') + '">text</p>')

    def test_active_content(self):
        for html in ('<script src="local.js"></script>', '<ScRiPt>alert(1)</ScRiPt>',
                     '<svg onload="alert(1)"></svg>', '<div ONCLICK="alert(1)">',
                     '<a href="java&#x09;script:alert(1)">bad</a>',
                     '<a href=" JAVASCRIPT:alert(1)">bad</a>',
                     '<iframe srcdoc="&lt;script&gt;alert(1)&lt;/script&gt;">',
                     '<object data="data:text/html;base64,PHNjcmlwdD4=">',
                     '<svg><set attributeName="href" to="https://remote.example"/></svg>',
                     '<meta http-equiv="refresh" content="0;url=https://remote.example">',
                     '<base href="https://remote.example/">', '<a href="#node1" ping="https://remote.example">'):
            with self.subTest(html=html):self.reject(html)

    def test_svg_required_and_no_runtime_reference(self):
        with self.assertRaises(ValueError):validate_published_html('<p>$x^2$</p>')
        self.reject('<link href="mathjax-tex-svg.js">')

    def test_review_payloads_fail_closed_in_both_paths(self):
        payloads = [
            '<img src=x onerror="alert(1)">',
            '<iframe srcdoc="&lt;script&gt;alert(1)&lt;/script&gt;"></iframe>',
            '<meta http-equiv="refresh" content="0;url=https://evil.invalid">',
            '<base href="https://evil.invalid/">',
            '<iframe src="../../payload.html"></iframe>',
            '<object data="payload.html"></object>', '<embed src="payload.svg">',
            '<link rel="stylesheet" href="payload.css">',
            '<a href="data:text/html,&lt;script&gt;alert(1)&lt;/script&gt;">click</a>',
            '<svg><feImage href="https://evil.invalid/x.svg"/></svg>',
            '<svg><text><textPath href="https://evil.invalid/p.svg#p">x</textPath></text></svg>',
            '<svg><rect filter="url(https://evil.invalid/f.svg#f)"/></svg>',
            '<svg><rect mask="url(other.svg#m)"/></svg>',
            '<svg><rect clip-path="url(other.svg#c)"/></svg>',
            '<svg><foreignObject><p>active namespace</p></foreignObject></svg>',
            '<form><input></form>', '<svg><g fill="url(#local)"/></svg>',
            '<div style="background:url(local.png)"></div>',
            '<div style="background:image-set(\'local.png\' 1x)"></div>',
            '<math><mtext><img src="x.png" onerror="alert(1)"></mtext></math>',
        ]
        for payload in payloads:
            for require_svg in (False,True):
                with self.subTest(payload=payload,math=require_svg),self.assertRaises(ValueError):
                    sanitize_published_html(document(self.SVG+payload),require_svg=require_svg)
        for css in ('@import "payload.css";', 'p { background:url(local.png) }',
                    r'p { background:u\72l(https://evil.invalid) }',
                    '@font-face { font-family:x; src:local(x) }'):
            with self.subTest(css=css),self.assertRaises(ValueError):
                sanitize_published_html(document(self.SVG, '<style>'+css+'</style>'))

    def test_caps_and_strict_structure(self):
        cases = ['<div>'*MAX_DEPTH+'</div>'*MAX_DEPTH,
                 '<br>'*MAX_TAGS, '<p title="'+'a'*(MAX_ATTRIBUTE+1)+'">x</p>',
                 '<p '+' '.join(f'data-x{i}="a"' for i in range(49))+'>x</p>',
                 '<svg><g></svg>', '<svg><title><p>bad namespace</p></title></svg>']
        for markup in cases:
            with self.subTest(size=len(markup)),self.assertRaises(ValueError):sanitize_published_html(document(markup))
        with self.assertRaises(ValueError):sanitize_published_html('x'*20_000_001)

    def test_real_head_first_child_and_idempotence(self):
        source='<!-- <head> -->'+document('<p>$x$</p>', '<meta charset="UTF-8"><title>Report</title>')
        safe,_=sanitize_published_html(source)
        self.assertTrue(safe.startswith('<!doctype html><html><head><meta http-equiv="Content-Security-Policy"'))
        self.assertNotIn('<!--',safe)
        self.assertEqual(sanitize_published_html(safe)[0],safe)
        class Policy(HTMLParser):
            content=None
            def handle_starttag(self,tag,attrs):
                attrs=dict(attrs)
                if tag=='meta' and 'http-equiv' in attrs:self.content=attrs['content']
        parser=Policy();parser.feed(safe);self.assertEqual(parser.content,DISK_CSP)


def document(body,head=''):
    return '<!doctype html><html><head>'+head+'</head><body>'+body+'</body></html>'


class FakeKernel:
    def __init__(self,runtime):
        self.lock=threading.RLock();self.runtime=runtime;self.generation=1
        self.state={}
    def snapshot(self):
        with self.lock:return dict(self.state)


class PublishLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='indymat-publish-security-')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve()
        self.app=App.__new__(App)
        self.app.port=12345;self.app.token='test-only'
        self.app.workspace=Workspace(self.root,self.root)
        self.app.file_lock=threading.Lock();self.app.publish_lock=threading.Lock()
        self.app.publish_renders={}
        self.app.kernel=FakeKernel(self.root/'jobs')
        self.job='a'*32;self.stage=self.root/'jobs'/self.job/'publish'
        self.stage.mkdir(parents=True)
        self.target=self.root/'html'/'report.html'
        _,initial=self.app.workspace.read_published(str(self.target),absent=True,create=True)
        self.cap=self.app.register_publish_render(self.job,self.target,1,initial)
        self.app.kernel.state={'kind':'publish','job':self.job,'status':'idle','error':None,
                               'publish':{'path':str(self.target),'name':'report.html','images':0}}

    def finish(self,source=None):
        if source is not None:(self.stage/'report.html').write_text(source)
        with self.app.kernel.lock:self.app.finish_publish(self.app.kernel)

    def get(self):
        return ReportCspTests.request(self.app,'/api/published-render?job='+self.job+'&render='+self.cap)

    def post(self):
        return ReportCspTests.request(self.app,'/api/published-render',
            {'job':self.job,'render':self.cap,'html':document(PublishValidationTests.SVG)})

    def test_fallback_and_writeback_are_sanitized_and_one_shot(self):
        self.finish(document('<p>$x^2$</p>'))
        self.assertIsNone(self.app.kernel.state['error'])
        self.assertFalse(self.stage.exists())
        self.assertIn('Content-Security-Policy',self.target.read_text())
        self.assertEqual(self.get()[0],200)
        self.assertEqual(self.post()[0],200)
        self.assertIn('<svg',self.target.read_text())
        self.assertIn('Content-Security-Policy',self.target.read_text())
        self.assertEqual(self.post()[0],403)

    def test_failed_fallback_is_not_installed_and_raw_stage_is_removed(self):
        self.finish(document('<img src=x onerror="alert(1)">'))
        self.assertIn('güvenli',self.app.kernel.state['error'])
        self.assertNotIn('publish',self.app.kernel.state)
        self.assertFalse(self.target.exists());self.assertFalse(self.stage.exists())
        self.assertEqual(self.get()[0],403)

    def test_failed_fallback_preserves_previous_safe_report(self):
        previous=sanitize_published_html(document('<p>Previous report</p>'))[0]
        self.target.write_text(previous)
        _,binding=self.app.workspace.read_published(str(self.target))
        self.app.publish_renders[self.cap]['initial']=binding
        self.finish(document('<iframe src="payload.html"></iframe>'))
        self.assertEqual(self.target.read_text(),previous)

    def test_target_symlink_before_get_cannot_redirect_capability(self):
        self.finish(document('<p>safe</p>'))
        victim=self.root/'victim.html';victim.write_text('do not touch')
        self.target.unlink();self.target.symlink_to(victim)
        self.assertEqual(self.get()[0],403)
        self.assertEqual(victim.read_text(),'do not touch')

    def test_same_bytes_new_inode_after_get_is_rejected(self):
        self.finish(document('<p>safe</p>'));self.assertEqual(self.get()[0],200)
        moved=self.target.with_name('original.html');self.target.rename(moved)
        self.target.write_bytes(moved.read_bytes())
        self.assertEqual(self.post()[0],409)
        self.assertEqual(self.target.read_bytes(),moved.read_bytes())

    def test_parent_symlink_even_to_original_directory_is_rejected(self):
        self.finish(document('<p>safe</p>'));self.assertEqual(self.get()[0],200)
        renamed=self.root/'old-html';self.target.parent.rename(renamed)
        self.target.parent.symlink_to(renamed,target_is_directory=True)
        self.assertEqual(self.post()[0],403)
        self.assertNotIn('<svg',(renamed/'report.html').read_text())

    def test_image_beside_destination_is_embedded_without_being_modified(self):
        png=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=')
        image=self.target.parent/'plot.png';image.write_bytes(png)
        self.finish(document('<img src="plot.png" alt="existing plot">'))
        self.assertIsNone(self.app.kernel.state['error'])
        self.assertEqual(image.read_bytes(),png)
        self.assertIn(base64.b64encode(png).decode(),self.target.read_text())

    def test_target_changed_while_octave_runs_is_not_overwritten(self):
        self.target.write_text('external file created while publishing')
        self.finish(document('<p>generated</p>'))
        self.assertIn('değişti',self.app.kernel.state['error'])
        self.assertEqual(self.target.read_text(),'external file created while publishing')
        self.assertFalse(self.stage.exists())

    def test_reset_or_new_job_between_validation_and_write_is_rejected(self):
        for reset in (True,False):
            with self.subTest(reset=reset):
                if not self.target.exists():self.finish(document('<p>safe</p>'))
                self.app.kernel.generation=1
                self.app.kernel.state.update(job=self.job,status='idle')
                self.app.publish_renders[self.cap]['used']=False
                self.assertEqual(self.get()[0],200);before=self.target.read_bytes()
                def validate_and_change(source):
                    safe=validate_published_html(source)
                    with self.app.kernel.lock:
                        if reset:self.app.kernel.generation=2
                        self.app.kernel.state.update(job='b'*32,status='running')
                    return safe
                with patch('app.validate_published_html',side_effect=validate_and_change):
                    self.assertEqual(self.post()[0],403)
                self.assertEqual(self.target.read_bytes(),before)
                self.assertTrue(self.app.publish_renders[self.cap]['used'])

    def test_rejected_writeback_consumes_ticket_but_keeps_fallback(self):
        self.finish(document('<p>safe</p>'));self.get();before=self.target.read_bytes()
        status,_,_=ReportCspTests.request(self.app,'/api/published-render',
            {'job':self.job,'render':self.cap,'html':document('<script>x</script>')})
        self.assertEqual(status,400);self.assertEqual(self.post()[0],403)
        self.assertEqual(self.target.read_bytes(),before)

    def test_reset_cannot_interleave_with_final_write(self):
        self.finish(document('<p>safe</p>'));self.get()
        entered=threading.Event();reset_done=threading.Event();threads=[]
        original=self.app.workspace.replace_published
        def write(*args,**kwargs):
            def reset():
                entered.set()
                with self.app.kernel.lock:self.app.kernel.generation+=1
                reset_done.set()
            worker=threading.Thread(target=reset);threads.append(worker);worker.start()
            self.assertTrue(entered.wait(1))
            self.assertFalse(reset_done.is_set(),'reset acquired the kernel lock inside final write')
            self.assertEqual(self.app.kernel.generation,1)
            return original(*args,**kwargs)
        with patch.object(self.app.workspace,'replace_published',side_effect=write):
            self.assertEqual(self.post()[0],200)
        for worker in threads:worker.join(1)
        self.assertTrue(reset_done.is_set())

    def test_final_component_swap_inside_replace_never_follows_victim(self):
        self.finish(document('<p>safe</p>'));self.get()
        victim=self.root/'victim.html';victim.write_text('victim unchanged')
        rename=os.rename
        def swap(source,destination,**kwargs):
            self.target.unlink();self.target.symlink_to(victim)
            return rename(source,destination,**kwargs)
        with patch('backend.files.os.rename',side_effect=swap):
            self.assertEqual(self.post()[0],403)
        self.assertEqual(victim.read_text(),'victim unchanged')

    def test_reset_discards_abandoned_raw_stage(self):
        (self.stage/'report.html').write_text(document('<iframe srcdoc="unsafe"></iframe>'))
        with self.app.kernel.lock:self.app.discard_publish_stages(self.app.kernel,1)
        self.assertFalse(self.stage.exists());self.assertFalse(self.target.exists())

    @unittest.skipUnless(shutil.which('octave-cli'),'octave-cli is not installed')
    def test_real_octave_report_content_and_raw_html(self):
        source=self.root/'report.m'
        source.write_text('%% Report $x^2$\n% $$e^{i\\pi}+1=0$$\n%% Items\n% * first item\n% * second item\n%\n% <html>\n% <img src="plot.png" alt="my plot">\n% <table><tr><td>raw table</td></tr></table>\n% </html>\ndisp("result < 42 & intact");\n')
        png=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=')
        (self.stage/'plot.png').write_bytes(png)
        quote_octave=lambda value:"'"+str(value).replace("'","''")+"'"
        root=Path(__file__).resolve().parents[1]
        code=f"addpath({quote_octave(root/'octave')});setappdata(0,'__mf_folder__',{quote_octave(self.stage.parent)});__mf_publish__({quote_octave(source)});"
        result=subprocess.run(['octave-cli','--quiet','--no-history','--eval',code],cwd=self.root,capture_output=True,text=True,timeout=45)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertFalse(self.target.exists(),'Octave must not install unvalidated HTML')
        raw=(self.stage/'report.html').read_text()
        self.finish();self.assertIsNone(self.app.kernel.state['error'],self.app.kernel.state['error'])
        saved=self.target.read_text()
        class Text(HTMLParser):
            def __init__(self):super().__init__();self.body=False;self.text=[]
            def handle_starttag(self,tag,attrs):
                if tag=='body':self.body=True
            def handle_endtag(self,tag):
                if tag=='body':self.body=False
            def handle_data(self,data):
                if self.body:self.text.append(data)
        before=Text();after=Text();before.feed(raw);after.feed(saved)
        self.assertEqual(''.join(before.text).encode(),''.join(after.text).encode(),'body text/code/output changed')
        for literal in ('<li>first item</li>','<li>second item</li>', '<td>raw table</td>',
                        '<pre class="oct-code">','<pre class="oct-code-output">result &lt; 42 &amp; intact',
                        '$x^2$', '$$e^{i\\pi}+1=0$$', '<a href="https://www.octave.org">'):
            self.assertIn(literal,raw);self.assertIn(literal,saved)
        self.assertIn('src="data:image/png;base64,'+base64.b64encode(png).decode()+'"',saved)
        self.assertNotIn('<script',saved)

    @unittest.skipUnless(shutil.which('node'),'node is not installed (development-only test)')
    def test_actual_mathjax_svg_is_accepted(self):
        root=Path(__file__).resolve().parents[1]
        # Use MathJax's local Node/lite adaptor, never a browser or a server JS
        # engine. The production page still performs all runtime rendering.
        script=r'''
require('mathjax').init({loader:{load:['input/tex','output/svg']},tex:{packages:['base','ams','newcommand','noundefined']},svg:{fontCache:'local'}}).then(m=>{
 const a=m.startup.adaptor;
 const tex=['x^2',String.raw`e^{i\pi}+1=0`,String.raw`\begin{pmatrix}1&2\\3&4\end{pmatrix}`,String.raw`\frac{a}{b}`,String.raw`\text{Türkçe}`];
 console.log(JSON.stringify(tex.map((t,i)=>a.outerHTML(m.tex2svg(t,{display:i!==0})))));
}).catch(e=>{console.error(e);process.exit(1)});
'''
        result=subprocess.run(['node','-e',script],cwd=root,capture_output=True,text=True,timeout=20)
        self.assertEqual(result.returncode,0,result.stderr)
        equations=json.loads(result.stdout)
        for equation in equations:
            safe=validate_published_html(document(equation))
            self.assertIn('<svg',safe);self.assertNotIn('<script',safe)

    def test_snapshot_adapter_finishes_before_exposing_success(self):
        (self.stage/'report.html').write_text(document('<p>staged</p>'))
        kernel=PublishKernel.__new__(PublishKernel)
        kernel.__dict__.update(self.app.kernel.__dict__)
        kernel.publish_app=self.app;kernel.breakpoints={}
        state=kernel.snapshot()
        self.assertIsNone(state['error']);self.assertTrue(self.target.exists())

    def test_collector_finishes_even_without_browser_polling(self):
        (self.stage/'report.html').write_text(document('<p>safe fallback without a page</p>'))
        (self.stage.parent/'result.json').write_text(json.dumps({'error':'','publish':self.app.kernel.state['publish']}))
        kernel=PublishKernel.__new__(PublishKernel)
        kernel.__dict__.update(self.app.kernel.__dict__)
        kernel.publish_app=self.app;kernel.breakpoints={};kernel.breakpoint_jobs={};kernel.closed=False
        kernel.state.update(status='running',started=time.time())
        events=queue.Queue();events.put(('__MF_DONE_'+self.job+'__').encode())
        kernel._collect(self.job,self.stage.parent,SimpleNamespace(poll=lambda:None),events,1,1)
        self.assertEqual(kernel.state['status'],'idle');self.assertIsNone(kernel.state['error'])
        self.assertIn('Content-Security-Policy',self.target.read_text())
        self.assertFalse(self.stage.exists())


class PublishClientTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which('node'),'Node is only needed for this development test')
    def test_limits_capability_lifecycle_and_auto_face(self):
        # Each assertion group imports only its feature's smallest public unit.
        # Bare dependencies never execute; permissive browser scaffolding is
        # separate from the strict spies on the behaviour under test.
        script=r'''
const assert=require('node:assert/strict'),vm=require('node:vm');
// Callable/constructible, chainable, empty when iterated, and NOT thenable.
// This is scaffolding, not a DOM emulator: layout remains a browser test.
function inert(overrides={}) {
 const target=Object.assign(function(){},overrides);
 let proxy;
 proxy=new Proxy(target,{
  get(target,key){
   if(Object.hasOwn(target,key))return Reflect.get(target,key);
   if(key==='then')return undefined;
   if(key===Symbol.iterator)return function*(){};
   if(key===Symbol.toPrimitive)return ()=>'';
   const child=inert();target[key]=child;return child;
  },
  apply(){return inert()},construct(){return inert()}
 });
 return proxy;
}
const thirdPartyBoundary={name:'inert-third-party',setup(build){
 build.onResolve({filter:/.*/},args=>{
  if(!args.path.startsWith('./')&&!args.path.startsWith('../'))return {path:args.path,external:true};
 });
}};
async function feature(name) {
 const source=`import shared from './frontend/state.js';import registry from './frontend/registry.js';
 import './frontend/shortcut_registry.js';import './frontend/${name}.js';globalThis.modules={shared,registry};
 // These assertions read the Turkish text; without a browser locale the default is English.
 import {setLanguage} from './frontend/i18n.js';setLanguage('tr');
 // Keep the isolation boundary exercised even before a feature imports these.
 import {EditorView} from '@codemirror/view';globalThis.packageProbe=EditorView;
 document.documentElement.style.testProbe;window.navigator.userAgent;document.createElement('div').style.testProbe;`;
 const built=await require('esbuild').build({stdin:{contents:source,resolveDir:process.cwd()},bundle:true,write:false,
  platform:'node',format:'cjs',plugins:[thirdPartyBoundary],metafile:true});
 assert(!Object.keys(built.metafile.inputs).some(p=>p.includes('node_modules/')),'third-party code entered the bundle');
 const imports=[],dependencies=new Map();
 const document=inert(),window=inert({document});
 const context={TextEncoder,console,confirm:()=>true,window,document,self:window,
  navigator:window.navigator,location:window.location,localStorage:inert(),sessionStorage:inert(),
  getComputedStyle:()=>inert(),MutationObserver:inert(),ResizeObserver:inert(),HTMLElement:inert(),
  Node:inert(),NodeFilter:{SHOW_TEXT:4},URL:inert(),setTimeout:()=>0,clearTimeout(){},
  setInterval:()=>0,clearInterval(){},requestAnimationFrame:()=>0,cancelAnimationFrame(){},
  require(specifier){
   assert(!specifier.startsWith('./')&&!specifier.startsWith('../'),'relative module escaped bundling');
   imports.push(specifier);
   if(!dependencies.has(specifier))dependencies.set(specifier,inert());
   return dependencies.get(specifier);
  }};
 vm.runInNewContext(built.outputFiles[0].text,context,{timeout:5000});
 assert(imports.includes('@codemirror/view'),'dependency stub was not exercised');
 const {registry:r,shared:s}=context.modules;
 // Cross-feature UI calls are irrelevant here; each tested seam is explicitly
 // overridden below with real state transitions or strict assertions/spies.
 Object.setPrototypeOf(r,inert());
 const nodes=new Map();
 r.$=key=>{if(!nodes.has(key))nodes.set(key,inert());return nodes.get(key)};
 r.el=()=>inert();
 return {r,s,context};
}
(async()=>{
let {r,s,context}=await feature('publish');
let inputs=[],mutations=0,toasts=[],blobs=0,posts=0;
context.document.createElement=()=>{throw Error('MathJax must not load for oversized reports')};
context.DOMParser=class {parseFromString(){return {body:{},querySelectorAll:()=>[],
 createTreeWalker(){let i=0;return {currentNode:null,nextNode(){this.currentNode=inputs[i++];return !!this.currentNode}}},
 createDocumentFragment(){mutations++;throw Error('oversized preflight mutated the document')}}}};
Object.assign(r,{toast:m=>toasts.push(m),modal(){},textAPI:async()=>'<sanitised source>',
 blobAPI:async()=>{blobs++;return 'blob:test'},api:async()=>{posts++}});
const node=text=>({data:text,parentElement:{closest:()=>false}});
 assert.equal(r.texRanges('$x$ and $$y$$').length,2);
 assert.equal(r.texRanges(String.raw`\$price $x$`).length,1);
 assert.equal(r.texRanges('$$unclosed $text').length,0);
 assert.equal(r.texRanges('$x$'.repeat(500)).length,500);
 assert.throws(()=>r.texRanges('$x$'.repeat(501)),/500 denklem/);
 assert.throws(()=>r.texRanges('$x$'.repeat(250000)),/500 denklem/);
 assert.equal(r.texRanges('$'+'a'.repeat(4096)+'$').length,1);
 assert.throws(()=>r.texRanges('$'+'a'.repeat(4097)+'$'),/4096 karakter/);
 assert.equal(r.texRanges(('$'+'ü'.repeat(2048)+'$').repeat(32)).length,32);
 assert.throws(()=>r.texRanges(('$'+'ü'.repeat(2048)+'$').repeat(33)),/128 KiB/);
 // Shared document budget spans text nodes; no partial transformation or POST.
 for(const oversized of [Array(501).fill('$x$'),['$'+'a'.repeat(4097)+'$'],Array(33).fill('$'+'ü'.repeat(2048)+'$')]){
  inputs=oversized.map(node);mutations=0;posts=0;blobs=0;toasts=[];
  await r.showPublished({path:'/html/report.html',name:'report.html',images:0,staged:false},'job','cap');
  assert.equal(mutations,0);assert.equal(posts,0);assert.equal(blobs,1);assert.equal(s.mathJaxPromise,null);
  assert.equal(toasts.length,1);assert.match(toasts[0],/kaynak TeX/);assert.match(toasts[0],/sınır/);
 }
 await assert.rejects(()=>r.showPublished({staged:true},'job','cap'),/henüz/);
 inputs=[{data:'$x$'.repeat(250000),parentElement:{closest:()=>true}}];
 assert.equal(r.preparePublishedMath('<code/output/link text>').items.length,0);
 console.log('CAPS PASS');
 ({r,s}=await feature('poll'));
 const noop=()=>{};
 Object.assign(r,{toast:noop,renderVariables:noop,refreshDebugEditor:noop,applyConsoleClear:noop,
  updateFigures:async()=>{},refreshFiles:async()=>{},setStatus:state=>{s.busy=['running','stopping'].includes(state.status);s.starting=state.status==='starting'},
  showPublished:async(report,job,cap)=>{assert.equal(cap,'cap');assert.equal(s.publishRenders.has(job),false);posts++}});
 const state={status:'idle',job:'job',kind:'publish',epoch:1,version:'test',packages:[],cwd:'/',current:'/',publish:{path:'/html/report.html'}};
 const checkedPoll=async()=>{await r.poll();assert.equal(s.wasDisconnected,false,'poll swallowed an unexpected error')};
 for(const status of ['idle','dead']){
  r.poll.completed=null;s.lastEpoch=1;s.lastJob='job';s.publishRenders.set('job','cap');posts=0;
  r.api=async()=>({...state,status,error:'publish failed'});await checkedPoll();assert.equal(s.publishRenders.size,0);assert.equal(posts,0);
 }
 r.poll.completed=null;s.publishRenders.set('job','cap');posts=0;r.api=async()=>state;
 await checkedPoll();assert.equal(posts,1);assert.equal(s.publishRenders.size,0);
 s.publishRenders.set('job','late response');await checkedPoll();assert.equal(s.publishRenders.size,0);
 s.publishRenders.set('newer-job','cap');await checkedPoll();assert.equal(s.publishRenders.get('newer-job'),'cap','stale poll removed new ticket');
 r.api=async()=>({...state,epoch:2,status:'starting',job:'reset'});await checkedPoll();assert.equal(s.publishRenders.size,0);
 // Exercise the actual reset callback, without running bootstrap's init.
 ({r,s}=await feature('bootstrap'));
 const handlers=new Map();r.on=(selector,fn)=>handlers.set(selector,fn);r.safe=noop;r.bootstrap();
 r.poll=noop;r.toast=noop;
 s.publishRenders.set('old','cap');r.api=async()=>{assert.equal(s.publishRenders.size,0)};
 await handlers.get('#reset')();assert.equal(s.publishRenders.size,0);
 // A late publish response from before reset must not repopulate the map.
 ({r,s}=await feature('console'));
 let resolve;r.api=()=>new Promise(done=>{resolve=done});r.requireIdle=noop;s.active={dirty:false,hash:'saved',path:'/probe.m'};
 const pending=r.runFileMode('publish','test');s.uiGeneration++;s.publishRenders.clear();resolve({job:'late',render:'cap'});await pending;assert.equal(s.publishRenders.size,0);
 console.log('LIFECYCLE PASS');
 ({r}=await feature('figures'));
 for(const background of ['#050719','#f1f4fa']){
  const palette={background,text:'#808080'},plot={palette};
  const series={kind:'line',line_style:'-',line_color:[1,0,0],marker_edge_color:[1,0,0],marker_face_color:[1,1,1]};
  const style=auto=>r.InteractiveFigure.prototype.seriesStyle.call(plot,{...series,marker_face_auto:auto});
  assert.equal(style(true).face,background);assert.equal(style(false).face,r.plotColor(series.marker_face_color,palette));
  assert.equal(style(true).line,style(false).line);assert.equal(style(true).edge,style(false).edge);
 }
 console.log('AUTO FACE PASS');
 console.log('CLIENT PASS: limits before rendering; fallback; terminal/reset/epoch cleanup; stale responses; auto-face background');
})().catch(e=>{console.error(e);process.exitCode=1});
'''
        result=subprocess.run(['node','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertIn('CLIENT PASS',result.stdout)



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':unittest.main(verbosity=2)
