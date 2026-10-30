"""Scoreboard: run IndyMAT's compatibility cases on the `indy` engine.

Each `%% <id> | <tags>` section of `uyumluluk/durumlar/*.m` becomes the
function file `uy_durum_NNNN.m` (same wrapping as IndyMAT's compat.py and
the RunMat measurement's kos.py), called inside try/catch by a small
runner script, one process per case. PASS = the case ran to the end
without error (IndyMAT's own rule). Helper folders are shared, never
isolated per case.

Usage (from the repository root):
  python3 -I motor/scripts/skor.py <config> [--jobs N] [--only id,id] [--bin path]
config: bare | yardimci (uyumluluk/yardimcilar) | tam (octave/compat + yardimcilar)
Outputs go to ~/.cache/indymat-motor-fleet/asama2/<config>/.
"""
import json
import os
import re
import secrets
import shutil
import subprocess
import sys
import time
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
OUT = Path.home() / '.cache' / 'indymat-motor-fleet' / 'asama2'
RUNMAT = Path.home() / '.cache' / 'runmat-deneme' / 'ozet.json'
HEADER = re.compile(r'^%%\s*([\w-]+)\s*\|\s*([^|]*?)\s*(?:\|\s*(.*?)\s*)?$')
TIMEOUT = 60


def q(v):
    return "'" + str(v).replace("'", "''") + "'"


def load_cases():
    cases = []
    for path in sorted((REPO / 'uyumluluk' / 'durumlar').glob('*.m')):
        lines = path.read_text(encoding='utf-8').splitlines()
        area = lines[0][2:].strip()
        cur = None
        for line in lines[1:]:
            if line.startswith('%%'):
                m = HEADER.match(line)
                cur = {'id': m.group(1), 'functions': m.group(2).split(), 'area_title': area,
                       'file': path.name, 'code': []}
                cases.append(cur)
            elif cur is not None:
                cur['code'].append(line)
    return cases


def area(fname):
    f = fname[:2]
    if f in ('10', '11', '12', '13', '14'):
        return 'dil'
    if f[0] == '2' or f in ('30', '31'):
        return 'matematik'
    if f[0] == '4':
        return 'metin'
    if f in ('50', '60'):
        return 'veri tipleri'
    if f[0] == '7':
        return 'dosya G/Ç'
    if f[0] in '89':
        return 'grafik'
    if f[0] == 'a':
        return 'kontrol'
    if f[0] == 'b':
        return 'sinyal'
    if f[0] == 'c':
        return 'istatistik'
    return f


AREAS = ['dil', 'matematik', 'metin', 'veri tipleri', 'dosya G/Ç', 'grafik', 'kontrol', 'sinyal', 'istatistik']


def paths_for(config):
    if config == 'bare':
        return []
    if config == 'yardimci':
        return [REPO / 'uyumluluk' / 'yardimcilar']
    if config == 'tam':
        return [REPO / 'octave' / 'compat', REPO / 'uyumluluk' / 'yardimcilar']
    raise SystemExit(f'unknown config {config}')


def run_one(binary, outdir, paths, index, case):
    d = outdir / 'kos' / f'{index:04d}'
    if d.exists():
        shutil.rmtree(d)
    d.mkdir(parents=True)
    name = f'uy_durum_{index:04d}'
    (d / f'{name}.m').write_text(f'function {name}()\n' + '\n'.join(case['code']) + '\nend\n', encoding='utf-8')
    nonce = secrets.token_hex(8)
    runner = f"""uy_st = 'OK'; uy_msg = ''; uy_idn = '';
fprintf('@@START\\t%s\\n', {q(nonce)});
try
  {name}();
catch uy_err
  uy_st = 'FAIL';
  try, uy_msg = strrep(strrep(strrep(uy_err.message, char(10), ' '), char(13), ' '), char(9), ' '); catch, uy_msg = '<message unreadable>'; end
  try, uy_idn = uy_err.identifier; catch, uy_idn = ''; end
end
fprintf('@@CASE\\t%s\\t%s\\t%s\\t%s\\n', {q(nonce)}, uy_st, uy_idn, uy_msg);
"""
    (d / 'calistir.m').write_text(runner, encoding='utf-8')
    argv = [str(binary), 'run', 'calistir.m', '--cd', str(d)]
    for p in paths:
        argv += ['--path', str(p)]
    env = dict(os.environ, HOME=str(OUT / 'ev'))
    t0 = time.time()
    try:
        p = subprocess.run(argv, cwd=d, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, timeout=TIMEOUT, start_new_session=True)
        out = p.stdout.decode('utf-8', 'replace')
        err = p.stderr.decode('utf-8', 'replace')
        rc = p.returncode
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or b'').decode('utf-8', 'replace')
        err = (e.stderr or b'').decode('utf-8', 'replace')
        rc = None
    dt = time.time() - t0
    res = None
    started = False
    for line in out.splitlines():
        part = line.split('\t')
        if len(part) >= 2 and part[1] == nonce:
            if part[0] == '@@START':
                started = True
            elif part[0] == '@@CASE' and len(part) >= 5:
                res = {'raw': part[2], 'identifier': part[3], 'message': '\t'.join(part[4:])}
    if res is None:
        res = {'raw': 'NOMARKER', 'identifier': '', 'message': (err.strip() or out.strip())[-600:],
               'timeout': rc is None, 'started': started, 'rc': rc}
    res.update(id=case['id'], file=case['file'], area=area(case['file']), seconds=round(dt, 3))
    res['panic'] = 'internal engine error' in err
    res['stderr_tail'] = err.strip()[-300:]
    shutil.rmtree(d, ignore_errors=True)
    return res


MISSING = re.compile(r"Undefined function or variable '([A-Za-z_][\w.]*)'")


def summarise(config, results, meta):
    base = json.load(open(REPO / 'uyumluluk' / 'baseline.json'))['durumlar']
    rm = {}
    if RUNMAT.exists():
        oz = json.load(open(RUNMAT))
        rm = oz.get('runmat-izole', {})
        ob = oz.get('octave-bare', {})
    else:
        ob = {}
    rows = defaultdict(Counter)
    for r in results:
        a = r['area']
        rows[a]['n'] += 1
        rows[a]['pass'] += r['raw'] == 'OK'
        rows[a]['oct'] += base.get(r['id']) == 'OK'
        rows[a]['rm'] += rm.get(r['id'], {}).get('status') == 'PASS'
        rows[a]['ob'] += ob.get(r['id'], {}).get('status') == 'PASS'
    lines = [f'## indy — {config}', '',
             f"Cases {meta['cases']}, wall {meta['wall_seconds']} s, panics {meta['panics']}, timeouts {meta['timeouts']}", '',
             '| Area | Cases | indy | Octave+IndyMAT (baseline) | RunMat 0.6.2 | Octave bare+yardimci |',
             '|---|---:|---:|---:|---:|---:|']
    tot = Counter()
    for a in AREAS + sorted(set(rows) - set(AREAS)):
        if a not in rows:
            continue
        c = rows[a]
        tot.update(c)
        lines.append(f"| {a} | {c['n']} | {c['pass']} | {c['oct']} | {c['rm']} | {c['ob']} |")
    lines.append(f"| **total** | {tot['n']} | **{tot['pass']}** | {tot['oct']} | {tot['rm']} | {tot['ob']} |")
    fails = [r for r in results if r['raw'] != 'OK']
    miss = Counter()
    for r in fails:
        m = MISSING.search(r['message'])
        if m:
            miss[m.group(1)] += 1
    lines += ['', '### Top missing names', '', '| Name | Cases |', '|---|---:|']
    lines += [f'| `{k}` | {v} |' for k, v in miss.most_common(40)]
    msgs = Counter()
    for r in fails:
        if MISSING.search(r['message']):
            continue
        m = re.sub(r"'[^']*'", "'…'", r['message'])
        m = re.sub(r'\d+', 'N', m)[:110]
        msgs[m] += 1
    lines += ['', '### Top other error messages', '', '| Message (normalised) | Cases |', '|---|---:|']
    pipe = '\\|'
    lines += [f"| {k.replace('|', pipe)} | {v} |" for k, v in msgs.most_common(30)]
    both = sum(1 for r in results if r['raw'] == 'OK' and base.get(r['id']) == 'OK')
    only_indy = [r['id'] for r in results if r['raw'] == 'OK' and base.get(r['id']) != 'OK']
    lines += ['', f'Pass in both indy and Octave+IndyMAT: {both}; only indy: {len(only_indy)} ({", ".join(only_indy[:40])})']
    return '\n'.join(lines) + '\n'


def main():
    config = sys.argv[1]
    args = sys.argv[2:]
    jobs = int(args[args.index('--jobs') + 1]) if '--jobs' in args else 6
    only = set(args[args.index('--only') + 1].split(',')) if '--only' in args else None
    binary = Path(args[args.index('--bin') + 1]) if '--bin' in args else REPO / 'motor' / 'target' / 'release' / 'indy'
    outdir = OUT / config
    outdir.mkdir(parents=True, exist_ok=True)
    (OUT / 'ev').mkdir(parents=True, exist_ok=True)
    paths = paths_for(config)
    cases = load_cases()
    todo = [(i, c) for i, c in enumerate(cases) if only is None or c['id'] in only]
    t0 = time.time()
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(lambda ic: run_one(binary, outdir, paths, ic[0], ic[1]), todo))
    meta = {'config': config, 'paths': [str(p) for p in paths], 'cases': len(cases), 'ran': len(todo),
            'wall_seconds': round(time.time() - t0, 1),
            'panics': sum(r['panic'] for r in results),
            'timeouts': sum(1 for r in results if r.get('timeout'))}
    (outdir / 'sonuc.json').write_text(json.dumps({'meta': meta, 'results': results}, ensure_ascii=False, indent=1),
                                       encoding='utf-8')
    shutil.rmtree(outdir / 'kos', ignore_errors=True)
    print(Counter(r['raw'] for r in results), meta)
    if only is None:
        text = summarise(config, results, meta)
        (outdir / 'ozet.md').write_text(text, encoding='utf-8')
        print(text)
    else:
        for r in results:
            if r['raw'] != 'OK':
                print(r['id'], '|', r['identifier'], '|', r['message'][:300], '|', r['stderr_tail'][-200:])


if __name__ == '__main__':
    main()
