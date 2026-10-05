"""Differential probes: run the same MATLAB code in real Octave and in the local MATLAB and compare.

Probes are in uyumluluk/yoklamalar/*.txt, one per line: 'id | code'. The code must leave its result in
the variable r (several statements allowed on the line). Each probe runs in its own function workspace;
the result is reduced to one canonical line by uyumluluk/yardimcilar/uy_ozet.m (class|size|content) or
to 'HATA' when it raises. Nothing here is expected in advance: the two engines are compared with each
other, so a wrong belief about MATLAB cannot enter the list.

  python3 scripts/fark.py            run both engines, write uyumluluk/farklar.json and docs/Farklar.md
  python3 scripts/fark.py --only ID-PREFIX   limit to probes whose id starts with the prefix
  python3 scripts/fark.py --octave-only      run Octave alone and compare with the recorded MATLAB results
                                             (uyumluluk/yoklama-matlab.json); for work without MATLAB at hand
  python3 scripts/fark.py --octave-only --adapted  also measure adapted bodies; write only farklar-uyarlanmis.json
  Add --output /tmp/measurement.json to keep adapted measurements outside the repository.
Short-lived helper processes only; the user's session is never touched.
"""
from __future__ import annotations
import argparse, hashlib, json, re, shutil, sys, tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root))
from scripts import compat
from backend.source_adapter import ADAPTER_VERSION, AdapterProfile, adapt_source, verify_package
from backend.kernel import cli_executable

BASE=root/'uyumluluk'
LINE=re.compile(r'^([a-z0-9][a-z0-9-]*)\s*\|\s*(.+)$')

def load(folder,prefix=''):
    probes=[];seen=set()
    for path in sorted(Path(folder).glob('*.txt')):
        group=''
        for number,line in enumerate(path.read_text(encoding='utf-8').splitlines(),1):
            if not line.strip():continue
            if line.startswith('#'):group=line.lstrip('# ').strip();continue
            match=LINE.match(line)
            if not match:raise ValueError(f'{path.name}:{number}: satır "kimlik | kod" olmalı.')
            if match.group(1) in seen:raise ValueError(f'{path.name}:{number}: yinelenen kimlik {match.group(1)}')
            seen.add(match.group(1))
            if match.group(1).startswith(prefix):probes.append({'id':match.group(1),'code':match.group(2).strip(),'group':group,'file':path.name})
    return probes

def runner(probes,folder):
    """One function file per probe (so a parse error stays local) plus a driver; code valid in both engines."""
    names=[]
    for index,probe in enumerate(probes):
        name=f'uy_yoklama_{index:04d}';names.append(name)
        (folder/f'{name}.m').write_text(f"function r = {name}()\n  r = [];\n  {probe['code']}\nend\n",encoding='utf-8')
    lines=["function uy_yoklama_surucu()","  adlar = {" + ' '.join(f"'{name}'" for name in names) + "};","  for k = 1:numel(adlar)",
           "    try","      r = feval(adlar{k});","      cikti = uy_ozet(r);","    catch hata","      cikti = ['HATA|' hata.identifier '|' strrep(hata.message, sprintf('\\n'), ' ')];","    end",
           "    fprintf('@@Y\\t%d\\t%s\\n', k, cikti);","  end","  fprintf('@@BITTI\\n');","end"]
    (folder/'uy_yoklama_surucu.m').write_text('\n'.join(lines)+'\n',encoding='utf-8')

def parse(text,count):
    result={}
    for line in text.splitlines():
        if line.startswith('@@Y\t'):
            _,index,value=line.split('\t',2);result[int(index)-1]=value
    return [result.get(index,'CALISMADI') for index in range(count)],'@@BITTI' in text

def run(probes,engine,timeout=900):
    with tempfile.TemporaryDirectory(prefix='uy-fark-') as temp:
        folder=Path(temp);runner(probes,folder)
        for helper in (BASE/'yardimcilar').glob('*.m'):(folder/helper.name).write_text(helper.read_text(encoding='utf-8'),encoding='utf-8')
        if engine=='matlab':
            (folder/'uy_fark_giris.m').write_text("uy_yoklama_surucu();\n",encoding='utf-8')
            text,_=compat.matlab_result(folder/'uy_fark_giris.m',folder,timeout)
        else:
            (folder/'uy_fark_giris.m').write_text(compat.setup()+"uy_yoklama_surucu();\n",encoding='utf-8')
            text=compat.octave(folder/'uy_fark_giris.m',folder,timeout)
        values,complete=parse(text or '',len(probes))
        if not complete:print(f'{engine}: sürücü tamamlanmadı; son çıktı: {(text or "")[-400:]}',file=sys.stderr)
        return values

NUMBER=re.compile(r'(?<![A-Za-z_])[-+]?(?:\d+\.?\d*(?:e[-+]?\d+)?|Inf|NaN)')

def close(matlab,octave,tolerance=1e-9):
    """True when two summaries differ only by rounding noise or the sign of zero.

    Character codes ('u65,66,') and everything that is not a number must match exactly.
    """
    protect=lambda text:re.sub(r'u(?:\d+,)*',lambda match:match.group(0).replace(',',';'),text)
    left,right=protect(matlab),protect(octave)
    if NUMBER.sub('#',left)!=NUMBER.sub('#',right):return False
    for a,b in zip(NUMBER.findall(left),NUMBER.findall(right)):
        if a==b:continue
        try:x,y=float(a),float(b)
        except ValueError:return False
        if x!=x or y!=y:
            if not (x!=x and y!=y):return False
        elif abs(x-y)>tolerance*max(1.0,abs(x),abs(y)):return False
    return True

def kind(matlab,octave):
    m_error,o_error=matlab.startswith('HATA|'),octave.startswith('HATA|')
    if m_error and o_error:return 'ikisi de hata'
    if matlab=='CALISMADI' or octave=='CALISMADI':return 'ölçülemedi'
    if o_error:return 'Octave hata verir'
    if m_error:return 'yalnız MATLAB hata verir'
    if matlab==octave:return 'aynı'
    if close(matlab,octave):return 'aynı'
    m,o=matlab.split('|',2),octave.split('|',2)
    if m[0]!=o[0]:return 'sınıf farklı'
    if m[1]!=o[1]:return 'boyut farklı'
    return 'değer farklı'

def report(rows,path):
    order=['Octave hata verir','sınıf farklı','boyut farklı','değer farklı','yalnız MATLAB hata verir','ölçülemedi']
    counts={name:sum(row['kind']==name for row in rows) for name in order+['aynı','ikisi de hata']}
    lines=['# MATLAB ve Octave farkları','',
           '`python3 scripts/fark.py` ile üretilir; elle düzenlenmez. Aynı kod gerçek MATLAB R2025b ve projenin Octave kurulumunda koşulur, sonuçlar birbiriyle karşılaştırılır.','',
           f"Toplam {len(rows)} yoklama: aynı {counts['aynı']}, ikisi de hata {counts['ikisi de hata']}, " + ', '.join(f"{name} {counts[name]}" for name in order) + '.','']
    for name in order:
        chosen=[row for row in rows if row['kind']==name]
        if not chosen:continue
        lines+=[f'## {name[0].upper()+name[1:]} ({len(chosen)})','','| Yoklama | Kod | MATLAB | Octave |','|---|---|---|---|']
        clip=lambda value:value.replace('|','¦')[:160]
        for row in chosen:lines.append(f"| `{row['id']}` | `{clip(row['code'])}` | {clip(row['matlab'])} | {clip(row['octave'])} |")
        lines.append('')
    Path(path).write_text('\n'.join(lines),encoding='utf-8')

RECORD=BASE/'yoklama-matlab.json'

def code_hash(probe):return hashlib.sha256(probe['code'].encode('utf-8')).hexdigest()[:16]

def recorded(probes):
    """MATLAB summaries recorded by the last full run; a probe whose code changed since has no record."""
    data=json.loads(RECORD.read_text(encoding='utf-8')) if RECORD.exists() else {}
    return [data[probe['id']]['matlab'] if data.get(probe['id'],{}).get('hash')==code_hash(probe) else 'CALISMADI' for probe in probes]

MATCH_KINDS = ('aynı', 'ikisi de hata')

def adapted_profile():
    executable = cli_executable(shutil.which('octave-cli') or '/opt/homebrew/bin/octave-cli')
    support = verify_package(root/'.packages/datatypes-1.5.0/string.m', executable=executable,
                             setup=compat.setup(), cwd=root)
    return AdapterProfile(enabled=True, package=support)


def adapt_probes(probes, profile):
    """Only bodies are adapted. Runner/summary helpers always remain native."""
    adaptations = [adapt_source(probe['code'], None, profile) for probe in probes]
    return [dict(probe, code=a.generated_text) for probe, a in zip(probes, adaptations)], adaptations


def adapted_measurement(probes, matlab, raw, adapted, adaptations):
    if len({len(probes), len(matlab), len(raw), len(adapted), len(adaptations)}) != 1:
        raise ValueError('Adapted measurement result counts do not match probe count.')
    rows = []
    for probe, m, o, a, adaptation in zip(probes, matlab, raw, adapted, adaptations):
        raw_kind, adapted_kind = kind(m, o), kind(m, a)
        rows.append({**probe, 'matlab': m, 'octave_raw': o, 'octave_adapted': a,
                     'raw_kind': raw_kind, 'adapted_kind': adapted_kind,
                     'original_hash': code_hash(probe),
                     'generated_hash': code_hash({'code': adaptation.generated_text}),
                     'adaptation': adaptation.metadata()})
    # A fallback executed the original unit. Second-run noise is recorded in
    # the row but cannot be credited/blamed as an adapter gain/regression.
    gained = [r for r in rows if r['adaptation']['status'] != 'fallback'
              and r['raw_kind'] not in MATCH_KINDS and r['adapted_kind'] in MATCH_KINDS]
    regressions = [r for r in rows if r['adaptation']['status'] != 'fallback'
                   and r['raw_kind'] in MATCH_KINDS and r['adapted_kind'] not in MATCH_KINDS]
    for row in regressions:
        names = sorted({d['message'].split(':', 1)[0] for d in row['adaptation']['diagnostics'] if d['code'] == 'semantic-limit'})
        row['regression_reason'] = ('String object behavior differs in ' + ', '.join(names) + '. '
                                    if names else 'String object dispatch differs. ') + row['octave_adapted']
        row['suggested_action'] = 'Verify the lexical callee/argument rule or use whole-unit fallback for unsupported object semantics; never coerce an expression or string array to char.'
    remaining = [r for r in rows if r['adapted_kind'] not in MATCH_KINDS]
    fallbacks = [r for r in rows if r['adaptation']['status'] == 'fallback']
    return {'adapter_version': ADAPTER_VERSION, 'probes': len(rows),
            'gained': len(gained), 'gained_successful': sum(r['adapted_kind'] == 'aynı' for r in gained),
            'gained_expected_error': sum(r['adapted_kind'] == 'ikisi de hata' for r in gained),
            'successful_matches': sum(r['adapted_kind'] == 'aynı' for r in rows),
            'expected_error_matches': sum(r['adapted_kind'] == 'ikisi de hata' for r in rows),
            'remaining': len(remaining), 'regressions': len(regressions), 'fallbacks': len(fallbacks),
            'regression_ids': [r['id'] for r in regressions], 'rows': rows}


def print_adapted(measurement):
    print(f"{measurement['probes']} probes: gained {measurement['gained']} "
          f"(successful {measurement['gained_successful']}, both-error {measurement['gained_expected_error']}), "
          f"remaining {measurement['remaining']}, NEW regressions {measurement['regressions']}, "
          f"fallbacks {measurement['fallbacks']}")
    print(f"Matches: successful {measurement['successful_matches']}, both-error {measurement['expected_error_matches']}")
    for row in measurement['rows']:
        if row['id'] in measurement['regression_ids']:
            print(f"REGRESSION {row['id']}: {row['adapted_kind']}; M: {row['matlab']}; "
                  f"raw: {row['octave_raw']}; adapted: {row['octave_adapted']}; action: {row['suggested_action']}")
        elif row['adapted_kind'] not in MATCH_KINDS:
            print(f"REMAINING {row['id']}: {row['adapted_kind']}; M: {row['matlab']}; adapted: {row['octave_adapted']}")
        if row['adaptation']['status'] == 'fallback':
            print(f"FALLBACK {row['id']}: " + '; '.join(d['message'] for d in row['adaptation']['diagnostics']))


def main():
    parser=argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--only',default='',metavar='ÖNEK')
    parser.add_argument('--adapted', action='store_true', help='compare opt-in adapted probe bodies and raw Octave separately')
    parser.add_argument('--output', type=Path, help='adapted measurement JSON destination (default: uyumluluk/farklar-uyarlanmis.json)')
    parser.add_argument('--octave-only',action='store_true',help='MATLAB yerine kayıtlı MATLAB sonuçlarını kullan')
    arguments=parser.parse_args()
    probes=load(BASE/'yoklamalar',arguments.only)
    if not probes:print('Yoklama yok.');return 0
    matlab=recorded(probes) if arguments.octave_only else run(probes,'matlab')
    octave=run(probes,'octave')
    if arguments.adapted:
        generated, adaptations = adapt_probes(probes, adapted_profile())
        adapted = run(generated, 'octave')
        measurement = adapted_measurement(probes, matlab, octave, adapted, adaptations)
        (arguments.output or BASE/'farklar-uyarlanmis.json').write_text(json.dumps(measurement, ensure_ascii=False, indent=1)+'\n', encoding='utf-8')
        print_adapted(measurement)
        return 0
    rows=[{**probe,'matlab':m,'octave':o,'kind':kind(m,o)} for probe,m,o in zip(probes,matlab,octave)]
    different=[row for row in rows if row['kind'] not in ('aynı','ikisi de hata')]
    if not arguments.only and not arguments.octave_only:
        RECORD.write_text(json.dumps({probe['id']:{'hash':code_hash(probe),'matlab':value} for probe,value in zip(probes,matlab)},ensure_ascii=False,indent=0,sort_keys=True)+'\n',encoding='utf-8')
        (BASE/'farklar.json').write_text(json.dumps({'yoklama':len(rows),'farklar':[{key:row[key] for key in ('id','code','group','kind','matlab','octave')} for row in different]},ensure_ascii=False,indent=1)+'\n',encoding='utf-8')
        (root/'docs').mkdir(exist_ok=True);report(rows,root/'docs'/'Farklar.md')
    print(f'{len(rows)} yoklama, {len(different)} fark')
    for row in different:print(f"{row['kind']:<26} {row['id']:<28} M: {row['matlab'][:70]}  O: {row['octave'][:70]}")
    return 0

if __name__=='__main__':raise SystemExit(main())
