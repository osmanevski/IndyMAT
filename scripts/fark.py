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

Serializer/reference/measurement schema 2: bounded summaries with omitted
content are partial evidence. Totals partition into successful matches,
both-error matches, observed remaining differences, partial and unmeasured.
Old probe caches require a fresh full run; --octave-only rejects them.
Fixture changes also require scripts/compat.py --matlab-reference.
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
SERIALIZER_SCHEMA = 2
REFERENCE_SCHEMA = 2
MEASUREMENT_SCHEMA = 2
PARTIAL_KIND = 'kısmi ölçüm'
DIFFERENCE_KINDS = ('Octave hata verir', 'yalnız MATLAB hata verir',
                    'sınıf farklı', 'boyut farklı', 'değer farklı')
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

    Character codes, integer/logical values and dimensions match exactly.
    Finite floating-point payloads alone receive numerical tolerance.
    """
    # Character codes, integer/logical payloads and dimensions are exact.
    protect=lambda text:re.sub(r'(?:u?int(?:8|16|32|64))\|[0-9x]+\|(?:-?\d+,)*|logical\|[0-9x]+\|[01]*|u(?:\d+,)*|i-?\d+,|\|(?:\d+x)*\d+\|',
        lambda match:re.sub(r'\d',lambda d:chr(97+int(d.group())),match.group()),text)
    left,right=protect(matlab),protect(octave)
    if NUMBER.sub('#',left)!=NUMBER.sub('#',right):return False
    for a,b in zip(NUMBER.findall(left),NUMBER.findall(right)):
        if a==b:continue
        try:x,y=float(a),float(b)
        except ValueError:return False
        if x!=x or y!=y:
            if not (x!=x and y!=y):return False
        elif abs(x)==float('inf') or abs(y)==float('inf'):
            if x!=y:return False
        elif abs(x-y)>tolerance*max(1.0,abs(x),abs(y)):return False
    return True

def coverage(summary):
    """Serialized value coverage; errors measure rejection only.

    Schema 2 has explicit <partial:reason> markers. Legacy opaque/truncated
    summaries remain partial evidence, never complete value equality.
    """
    if summary == 'CALISMADI':return {'status': 'missing', 'complete': False, 'reasons': ['not-run']}
    if summary.startswith('HATA|'):return {'status': 'error', 'complete': True, 'reasons': []}
    reasons = re.findall(r'<partial:([^>]+)>', summary)
    if '<nesne>' in summary:reasons.append('legacy-object')
    if '...' in summary:reasons.append('legacy-truncation')
    if re.search(r'(?:^|[={;])struct\|(?!1x1\|)[0-9x]+\|', summary):
        reasons.append('structure-array')
    return {'status': 'partial' if reasons else 'complete', 'complete': not reasons,
            'reasons': sorted(set(reasons))}

def kind(matlab,octave):
    m_error,o_error=matlab.startswith('HATA|'),octave.startswith('HATA|')
    if m_error and o_error:return 'ikisi de hata'
    if matlab=='CALISMADI' or octave=='CALISMADI':return 'ölçülemedi'
    if o_error:return 'Octave hata verir'
    if m_error:return 'yalnız MATLAB hata verir'
    m,o=matlab.split('|',2),octave.split('|',2)
    if m[0]!=o[0]:return 'sınıf farklı'
    if m[1]!=o[1]:return 'boyut farklı'
    mc,oc=coverage(matlab),coverage(octave)
    equal = matlab==octave or close(matlab,octave)
    if not mc['complete'] or not oc['complete']:
        # Unequal equally bounded samples prove an observed gap. Different
        # coverage boundaries cannot be compared as complete values.
        if not equal and mc['status']==oc['status']=='partial' and mc['reasons']==oc['reasons']:
            return 'değer farklı'
        return PARTIAL_KIND
    if equal:return 'aynı'
    return 'değer farklı'

def report(rows,path):
    order=['Octave hata verir','sınıf farklı','boyut farklı','değer farklı','yalnız MATLAB hata verir',PARTIAL_KIND,'ölçülemedi']
    counts={name:sum(row['kind']==name for row in rows) for name in order+['aynı','ikisi de hata']}
    lines=['# MATLAB ve Octave farkları','',
           '`python3 scripts/fark.py` ile üretilir; elle düzenlenmez. Aynı kod gerçek MATLAB R2025b ve projenin Octave kurulumunda koşulur, sonuçlar birbiriyle karşılaştırılır.','',
           f"Toplam {len(rows)} yoklama: aynı {counts['aynı']}, ikisi de hata {counts['ikisi de hata']}, " + ', '.join(f"{name} {counts[name]}" for name in order) + '.','']
    for name in order:
        chosen=[row for row in rows if row['kind']==name]
        if not chosen:continue
        lines+=[f'## {name[0].upper()+name[1:]} ({len(chosen)})','','| Yoklama | Kod | MATLAB | Octave | Kapsam |','|---|---|---|---|---|']
        clip=lambda value:value.replace('|','¦')[:160]
        for row in chosen:
            scope = json.dumps({'matlab': coverage(row['matlab']), 'octave': coverage(row['octave'])}, ensure_ascii=False)
            lines.append(f"| `{row['id']}` | `{clip(row['code'])}` | {clip(row['matlab'])} | {clip(row['octave'])} | {scope} |")
        lines.append('')
    Path(path).write_text('\n'.join(lines),encoding='utf-8')

RECORD=BASE/'yoklama-matlab.json'

def code_hash(probe):return hashlib.sha256(probe['code'].encode('utf-8')).hexdigest()[:16]

def reference_context(fixtures=None):
    return {'schema': REFERENCE_SCHEMA, 'serializer_schema': SERIALIZER_SCHEMA,
            'reference_fixture_fingerprint': compat.fixture_hash(fixtures or BASE/'yardimcilar')}

def source_identity(probe, context):
    """Full digest binds exact executable source, id, schema and all fixtures."""
    value = {**context, 'id': probe['id'], 'code': probe['code']}
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True,
                                    separators=(',', ':')).encode('utf-8')).hexdigest()

def reference_data(probes, matlab, fixtures=None):
    if len(probes) != len(matlab):raise ValueError('Reference result count differs from probe count.')
    context = reference_context(fixtures)
    return {**context, 'records': {p['id']: {'code': p['code'],
            'source_identity': source_identity(p, context), 'matlab': m}
            for p, m in zip(probes, matlab)}}

def recorded(probes):
    """Reject legacy schema, stale serializer/fixtures and changed exact source."""
    data=json.loads(RECORD.read_text(encoding='utf-8')) if RECORD.exists() else {}
    context = reference_context()
    if any(data.get(k) != v for k,v in context.items()):return ['CALISMADI'] * len(probes)
    records = data.get('records', {})
    values = []
    for probe in probes:
        record = records.get(probe['id'], {})
        valid = record.get('code') == probe['code'] and record.get('source_identity') == source_identity(probe, context)
        value = record.get('matlab')
        values.append(value if valid and isinstance(value, str) else 'CALISMADI')
    return values

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
    context = reference_context()
    for probe, m, o, a, adaptation in zip(probes, matlab, raw, adapted, adaptations):
        raw_kind, adapted_kind = kind(m, o), kind(m, a)
        rows.append({**probe, 'matlab': m, 'octave_raw': o, 'octave_adapted': a,
                     'raw_kind': raw_kind, 'adapted_kind': adapted_kind,
                     'original_hash': code_hash(probe),
                     'source_identity': source_identity(probe, context),
                     'coverage': {'matlab': coverage(m), 'octave_raw': coverage(o), 'octave_adapted': coverage(a)},
                     'generated_hash': code_hash({'code': adaptation.generated_text}),
                     'adaptation': adaptation.metadata()})
    # A fallback executed the original unit. Second-run noise is recorded in
    # the row but cannot be credited/blamed as an adapter gain/regression.
    gained = [r for r in rows if r['adaptation']['status'] != 'fallback'
              and r['raw_kind'] in DIFFERENCE_KINDS and r['adapted_kind'] in MATCH_KINDS]
    regressions = [r for r in rows if r['adaptation']['status'] != 'fallback'
                   and r['raw_kind'] in MATCH_KINDS and r['adapted_kind'] in DIFFERENCE_KINDS]
    for row in regressions:
        names = sorted({d['message'].split(':', 1)[0] for d in row['adaptation']['diagnostics'] if d['code'] == 'semantic-limit'})
        row['regression_reason'] = ('String object behavior differs in ' + ', '.join(names) + '. '
                                    if names else 'String object dispatch differs. ') + row['octave_adapted']
        row['suggested_action'] = 'Verify the lexical callee/argument rule or use whole-unit fallback for unsupported object semantics; never coerce an expression or string array to char.'
    remaining = [r for r in rows if r['adapted_kind'] in DIFFERENCE_KINDS]
    partial = [r for r in rows if r['adapted_kind'] == PARTIAL_KIND]
    unmeasured = [r for r in rows if r['adapted_kind'] == 'ölçülemedi']
    fallbacks = [r for r in rows if r['adaptation']['status'] == 'fallback']
    return {**context, 'measurement_schema': MEASUREMENT_SCHEMA,
            'adapter_version': ADAPTER_VERSION, 'probes': len(rows),
            'gained': len(gained), 'gained_successful': sum(r['adapted_kind'] == 'aynı' for r in gained),
            'gained_expected_error': sum(r['adapted_kind'] == 'ikisi de hata' for r in gained),
            'successful_matches': sum(r['adapted_kind'] == 'aynı' for r in rows),
            'expected_error_matches': sum(r['adapted_kind'] == 'ikisi de hata' for r in rows),
            'remaining': len(remaining), 'regressions': len(regressions), 'fallbacks': len(fallbacks),
            'partial': len(partial), 'unmeasured': len(unmeasured),
            'coverage_losses': sum(r['raw_kind'] in MATCH_KINDS and r['adapted_kind'] in (PARTIAL_KIND, 'ölçülemedi') for r in rows),
            'regression_ids': [r['id'] for r in regressions], 'rows': rows}


def print_adapted(measurement):
    print(f"{measurement['probes']} probes: gained {measurement['gained']} "
          f"(successful {measurement['gained_successful']}, both-error {measurement['gained_expected_error']}), "
          f"remaining differences {measurement['remaining']}, partial {measurement['partial']}, "
          f"unmeasured {measurement['unmeasured']}, NEW regressions {measurement['regressions']}, "
          f"fallbacks {measurement['fallbacks']}")
    print(f"Matches: successful {measurement['successful_matches']}, both-error {measurement['expected_error_matches']}")
    for row in measurement['rows']:
        if row['id'] in measurement['regression_ids']:
            print(f"REGRESSION {row['id']}: {row['adapted_kind']}; M: {row['matlab']}; "
                  f"raw: {row['octave_raw']}; adapted: {row['octave_adapted']}; action: {row['suggested_action']}")
        elif row['adapted_kind'] not in MATCH_KINDS:
            print(f"REMAINING {row['id']}: {row['adapted_kind']}; M: {row['matlab']}; adapted: {row['octave_adapted']}")
            print('COVERAGE ' + json.dumps(row['coverage'], ensure_ascii=False))
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
    rows=[{**probe,'matlab':m,'octave':o,'kind':kind(m,o),
           'coverage': {'matlab': coverage(m), 'octave': coverage(o)}} for probe,m,o in zip(probes,matlab,octave)]
    different=[row for row in rows if row['kind'] not in ('aynı','ikisi de hata')]
    if not arguments.only and not arguments.octave_only:
        RECORD.write_text(json.dumps(reference_data(probes,matlab),ensure_ascii=False,indent=0,sort_keys=True)+'\n',encoding='utf-8')
        (BASE/'farklar.json').write_text(json.dumps({**reference_context(), 'measurement_schema': MEASUREMENT_SCHEMA,
            'yoklama':len(rows), 'counts': {k: sum(r['kind']==k for r in rows) for k in (*MATCH_KINDS, *DIFFERENCE_KINDS, PARTIAL_KIND, 'ölçülemedi')},
            'farklar':[{key:row[key] for key in ('id','code','group','kind','matlab','octave','coverage')} for row in different]},ensure_ascii=False,indent=1)+'\n',encoding='utf-8')
        (root/'docs').mkdir(exist_ok=True);report(rows,root/'docs'/'Farklar.md')
    print(f"{len(rows)} yoklama, {sum(r['kind'] in DIFFERENCE_KINDS for r in rows)} fark, "
          f"{sum(r['kind']==PARTIAL_KIND for r in rows)} kısmi ölçüm, {sum(r['kind']=='ölçülemedi' for r in rows)} ölçülemedi")
    for row in different:print(f"{row['kind']:<26} {row['id']:<28} M: {row['matlab'][:70]}  O: {row['octave'][:70]}")
    return 0

if __name__=='__main__':raise SystemExit(main())
