"""Differential probes: run the same MATLAB code in real Octave and in the local MATLAB and compare.

Probes are in uyumluluk/yoklamalar/*.txt, one per line: 'id | code'. The code must leave its result in
the variable r (several statements allowed on the line). Each probe runs in its own function workspace;
the result is reduced to one canonical line by uyumluluk/yardimcilar/uy_ozet.m (class|size|content) or
to 'HATA' when it raises. Nothing here is expected in advance: the two engines are compared with each
other, so a wrong belief about MATLAB cannot enter the list.

  python3 scripts/fark.py            run both engines, write uyumluluk/farklar.json and docs/Farklar.md
  python3 scripts/fark.py --only ID-PREFIX   limit to probes whose id starts with the prefix
Short-lived helper processes only; the user's session is never touched.
"""
from __future__ import annotations
import argparse, json, re, sys, tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root))
from scripts import compat

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

def main():
    parser=argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--only',default='',metavar='ÖNEK')
    arguments=parser.parse_args()
    probes=load(BASE/'yoklamalar',arguments.only)
    if not probes:print('Yoklama yok.');return 0
    matlab=run(probes,'matlab');octave=run(probes,'octave')
    rows=[{**probe,'matlab':m,'octave':o,'kind':kind(m,o)} for probe,m,o in zip(probes,matlab,octave)]
    different=[row for row in rows if row['kind'] not in ('aynı','ikisi de hata')]
    if not arguments.only:
        (BASE/'farklar.json').write_text(json.dumps({'yoklama':len(rows),'farklar':[{key:row[key] for key in ('id','code','group','kind','matlab','octave')} for row in different]},ensure_ascii=False,indent=1)+'\n',encoding='utf-8')
        (root/'docs').mkdir(exist_ok=True);report(rows,root/'docs'/'Farklar.md')
    print(f'{len(rows)} yoklama, {len(different)} fark')
    for row in different:print(f"{row['kind']:<26} {row['id']:<28} M: {row['matlab'][:70]}  O: {row['octave'][:70]}")
    return 0

if __name__=='__main__':raise SystemExit(main())
