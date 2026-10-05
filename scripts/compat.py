"""Function inventory and behaviour cases against the real project Octave.

Runs short-lived octave-cli processes only; the user's session is never touched.
Presence of a function is not proof of its behaviour: only passing cases count as verified.
"""
from __future__ import annotations
import argparse, hashlib, json, os, re, secrets, shutil, subprocess, sys, tempfile, signal, threading
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root))
from backend.kernel import cli_executable, _Detached

BASE=root/'uyumluluk'
REPORT=root/'docs'/'Uyumluluk-Envanteri.md'
PACKAGES=('control','signal','datatypes')
# Functions that exist only as methods of these classes are reported as present.
CLASSES=('lti','tf','ss','zpk','frd','iddata','table','datetime','duration','calendarDuration','categorical','string','containers.Map','inputParser')
PACKAGE_TITLES={'control':'Kontrol sistemleri','signal':'Sinyal işleme','communications':'Haberleşme','datatypes':'Veri tipleri ve kapsayıcılar','dicom':'DICOM','financial':'Finans','image':'Görüntü işleme','statistics':'İstatistik','symbolic':'Sembolik matematik','optim':'Optimizasyon'}
UNSORTED='Çekirdek: sınıflandırılmamış'
HEADER=re.compile(r'^%%\s*([\w-]+)\s*\|\s*([^|]*?)\s*(?:\|\s*(.*?)\s*)?$')
NAME=r'[A-Za-z]\w*(?:\.\w+)*'
MATLAB_SCHEMA=2
MATLAB_VERSION=re.compile(r'\d+(?:\.\d+){2,3} \(R\d{4}[ab]\)(?: Update \d+)?')

def quote(value):return "'"+str(value).replace("'","''")+"'"

def setup():
    """Same path and package state as the application session (see Kernel.start)."""
    packages=root/'.packages'
    code="more off; warning('off','Octave:shadowed-function'); set(0,'defaultfigurevisible','off'); "
    code+=f"addpath({quote(root/'octave'/'compat')}); "
    if (packages/'octave_packages').exists():
        code+=f"pkg('prefix',{quote(packages)},{quote(packages/'.arch')}); pkg('local_list',{quote(packages/'octave_packages')}); "
        code+=''.join(f"try, pkg('load','{name}'); catch, end_try_catch; " for name in PACKAGES)
    return code+'\n'

def run_process(command,cwd,timeout):
    try:
        done=subprocess.run(command,cwd=cwd,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,encoding='utf-8',errors='replace',timeout=timeout,start_new_session=True)
        return done.stdout,done.returncode
    except subprocess.TimeoutExpired as exc:
        partial=exc.stdout or ''
        return (partial.decode('utf-8','replace') if isinstance(partial,bytes) else partial),None

def octave_result(script,cwd,timeout):
    exe=cli_executable(os.environ.get('MATLAB_FREE_OCTAVE') or shutil.which('octave') or '/opt/homebrew/bin/octave')
    return run_process([exe,'--no-gui','--quiet','--no-init-file','--no-site-file','--no-history',str(script)],cwd,timeout)

def octave(script,cwd,timeout):return octave_result(script,cwd,timeout)[0]

def octave_cases_result(script,cwd,timeout):
    """Behaviour cases run under the Qt build, like the application session, so figures can be printed while invisible.

    On macOS the process is detached (backend.kernel._Detached) to stay out of the Dock; its exit
    status is then unknown, so completeness is judged by the runner's final marker alone.
    """
    argv=[os.environ.get('MATLAB_FREE_OCTAVE') or shutil.which('octave') or '/opt/homebrew/bin/octave','--no-gui','--quiet','--no-init-file','--no-site-file','--no-history',str(script)]
    if sys.platform!='darwin':return run_process(argv,cwd,timeout)
    proc=_Detached(argv,cwd);proc.stdin.close();chunks=[]
    reader=threading.Thread(target=lambda:chunks.append(proc.stdout.read()),daemon=True);reader.start();reader.join(timeout)
    timed_out=reader.is_alive()
    if timed_out:
        try:os.killpg(proc.pid,signal.SIGKILL)
        except ProcessLookupError:pass
        reader.join(5)
    return b''.join(chunk or b'' for chunk in chunks).decode('utf-8','replace'),(None if timed_out else 0)

def matlab_executable():
    """A local MATLAB is optional; it is only used to validate the expectations written in the cases."""
    found=os.environ.get('INDYMAT_MATLAB') or shutil.which('matlab')
    if found:return found
    installed=sorted(Path('/Applications').glob('MATLAB_R*.app/bin/matlab'))
    return str(installed[-1]) if installed else None

def matlab_result(script,cwd,timeout):return run_process([matlab_executable(),'-batch',Path(script).stem],cwd,timeout)

def matlab(script,cwd,timeout):return matlab_result(script,cwd,timeout)[0]

def curated_reference(folder):
    """[(title, names)] from 'referans/*.txt': first line '# title', then whitespace separated names."""
    areas=[]
    for path in sorted(Path(folder).glob('*.txt')):
        lines=path.read_text(encoding='utf-8').splitlines()
        if not lines or not lines[0].startswith('# '):raise ValueError(f'{path.name}: ilk satır "# başlık" olmalı.')
        names=[name for line in lines[1:] if not line.startswith('#') for name in line.split()]
        bad=[name for name in names if not re.fullmatch(NAME,name)]
        if bad:raise ValueError(f'{path.name}: geçersiz ad {bad[0]}')
        areas.append((lines[0][2:].strip(),names))
    return areas

def octave_reference(path):
    """Octave's own list of MATLAB functions: per-package groups and the core functions it knows are missing."""
    text=Path(path).read_text(encoding='utf-8',errors='replace')
    names=lambda block:re.findall(f'"({NAME})"',block)
    groups=[(package,names(block)) for block,package in re.findall(r'case\s*\{([^}]*)\}\s*txt = check_package \(fcn, "([^"]+)"',text)]
    core=re.search(r'persistent list = \{(.*?)\};',text,re.S)
    if not groups or not core:raise ValueError('Octave eksik fonksiyon listesi beklenen biçimde değil.')
    return groups,names(core.group(1))

def build_reference(curated,groups,core):
    """Ordered {name: area}; a curated area wins over Octave's grouping."""
    reference={}
    for title,names in curated:
        for name in names:reference.setdefault(name,title)
    for package,names in groups:
        for name in names:reference.setdefault(name,PACKAGE_TITLES.get(package,package))
    for name in core:reference.setdefault(name,UNSORTED)
    return reference

def executable_text(code):
    """Mask comments and quoted text, preserving adjacent transpose operators."""
    out=[];i=0
    while i<len(code):
        char=code[i]
        if char=='%':
            end=code.find('\n',i)
            if end<0:break
            out.append('\n');i=end+1;continue
        transpose=char=="'" and i>0 and (code[i-1].isalnum() or code[i-1] in "_)]}.'")
        if char in "\"'" and not transpose:
            delimiter=char;out.append(' ');i+=1
            while i<len(code):
                if code[i]==delimiter:
                    i+=1
                    if i<len(code) and code[i]==delimiter:i+=1;continue
                    break
                if code[i]=='\n':out.append('\n')
                i+=1
            continue
        out.append(char);i+=1
    return ''.join(out)

def load_cases(folder):
    """Cases from 'durumlar/*.m': first line '% area title', then '%% id | functions | note' blocks."""
    cases=[];seen=set()
    for path in sorted(Path(folder).glob('*.m')):
        lines=path.read_text(encoding='utf-8').splitlines()
        if not lines or not lines[0].startswith('% '):raise ValueError(f'{path.name}: ilk satır "% alan başlığı" olmalı.')
        area=lines[0][2:].strip();current=None;file_cases=[]
        for number,line in enumerate(lines[1:],2):
            if line.startswith('%%'):
                match=HEADER.match(line)
                if not match or not match.group(2):raise ValueError(f'{path.name}:{number}: başlık "%% kimlik | fonksiyonlar" olmalı.')
                if match.group(1) in seen:raise ValueError(f'{path.name}:{number}: yinelenen kimlik {match.group(1)}')
                seen.add(match.group(1))
                current={'id':match.group(1),'functions':match.group(2).split(),'note':match.group(3) or '','area':area,'code':[]}
                cases.append(current);file_cases.append(current)
            elif current is not None:current['code'].append(line)
            elif line.strip():raise ValueError(f'{path.name}:{number}: ilk durum başlığından önce kod var.')
        for case in file_cases:
            code=executable_text('\n'.join(case['code']))
            for name in case['functions']:
                if name.startswith(':'):continue
                identifier=name.rsplit('.',1)[-1]
                if not re.search(rf'(?<![A-Za-z0-9_]){re.escape(identifier)}(?![A-Za-z0-9_])',code):
                    raise ValueError(f"{path.name}: {case['id']}: başlıktaki {name} kodda kullanılmıyor.")
    return cases

def fixture_hash(folder=BASE/'yardimcilar'):
    """Hash relative file names and bytes, independently of tree location and traversal order."""
    folder=Path(folder);digest=hashlib.sha256()
    for path in sorted((p for p in folder.rglob('*') if p.is_file()),key=lambda p:p.relative_to(folder).as_posix()):
        for value in (path.relative_to(folder).as_posix().encode('utf-8'),path.read_bytes()):
            digest.update(len(value).to_bytes(8,'big'));digest.update(value)
    return digest.hexdigest()

def case_hash(case,fixtures=BASE/'yardimcilar'):
    """Stable identity of executable expectations; the explanatory note is deliberately excluded."""
    value={'code':'\n'.join(case['code']),'functions':case['functions'],'id':case['id'],'fixtures':fixture_hash(fixtures)}
    return hashlib.sha256(json.dumps(value,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode('utf-8')).hexdigest()

def matlab_record(case,status,fixtures=BASE/'yardimcilar'):return {'hash':case_hash(case,fixtures),'status':status}

def matlab_metadata_valid(data):
    return type(data.get('schema')) is int and data['schema']==MATLAB_SCHEMA and isinstance(data.get('matlab'),str) and bool(MATLAB_VERSION.fullmatch(data['matlab']))

def confirmed_matlab_cases(cases,data,fixtures=BASE/'yardimcilar'):
    if not matlab_metadata_valid(data):return set()
    records=data.get('durumlar',{})
    return {case['id'] for case in cases if records.get(case['id'])==matlab_record(case,'OK',fixtures)}

def matlab_reference_valid(cases,data,fixtures=BASE/'yardimcilar'):
    if not matlab_metadata_valid(data):return False
    expected={case['id']:matlab_record(case,'OK',fixtures) for case in cases}
    return data.get('durumlar')==expected

def probe(names,folder):
    """Ask Octave which names exist, where they come from and which are class methods."""
    folder=Path(folder);(folder/'names.txt').write_text('\n'.join(names)+'\n',encoding='utf-8')
    body=r"""
printf("@@VERSION\t%s\n",version());
printf("@@UNIMPLEMENTED\t%s\n",which('__unimplemented__'));
installed=pkg('list');
for k=1:numel(installed), if installed{k}.loaded, printf("@@PACKAGE\t%s %s\n",installed{k}.name,installed{k}.version); endif, endfor
names=strsplit(strtrim(fileread('names.txt')),"\n");
for k=1:numel(names)
  name=names{k}; if isempty(name), continue, endif
  found=exist(name); source='';
  % 'which' is slow for unknown names; only packaged names such as containers.Map need it then.
  if found>0 || any(name=='.'), try, source=which(name); catch, end_try_catch, endif
  if ~ischar(source), source=''; endif
  printf("@@FN\t%s\t%d\t%s\n",name,found,source);
endfor
"""
    body+=f"classes={{{','.join(quote(name) for name in CLASSES)}}};\n"
    body+=r"""for k=1:numel(classes)
  try, list=methods(classes{k}); printf("@@METHODS\t%s\t%s\n",classes{k},strjoin(list(:)',' ')); catch, end_try_catch
endfor
"""
    (folder/'yokla.m').write_text(setup()+body,encoding='utf-8')
    result={'version':'','unimplemented':'','packages':[],'functions':{},'methods':{}}
    for line in octave(folder/'yokla.m',folder,120).splitlines():
        part=line.split('\t')
        if part[0]=='@@VERSION':result['version']=part[1]
        elif part[0]=='@@UNIMPLEMENTED':result['unimplemented']=part[1]
        elif part[0]=='@@PACKAGE':result['packages'].append(part[1])
        elif part[0]=='@@FN' and len(part)==4:result['functions'][part[1]]={'exist':int(part[2]),'source':part[3]}
        elif part[0]=='@@METHODS' and len(part)==3:
            for name in part[2].split():result['methods'].setdefault(name,[]).append(part[1])
    if not result['version']:raise RuntimeError('Octave yoklaması yanıt vermedi.')
    return result

def run_cases(cases,folder,fixtures,timeout=240):
    return execute(cases,folder,fixtures,timeout)[0]

def parse_execution_output(output,cases,nonce,returncode=0):
    """Commit the last result at the next START or DONE; an unclosed final case is inconclusive."""
    ids=[case['id'] for case in cases];results={};started=None;pending=None;count=0;complete=False;version='';exists={}
    for line in output.splitlines():
        part=line.split('\t')
        if len(part)<2 or part[1]!=nonce:continue
        if part[0]=='@@VERSION' and len(part)==3 and not version:version=part[2]
        elif part[0]=='@@FN' and len(part)==4:
            try:exists[part[2]]=int(part[3])
            except ValueError:pass
        elif part[0]=='@@START' and len(part)==3 and count<len(ids) and part[2]==ids[count]:
            if pending is not None:results[started]=pending
            started=part[2];pending=None;count+=1
        elif part[0]=='@@CASE' and len(part)>=4 and part[2]==started and part[3] in ('OK','FAIL'):
            pending={'status':part[3],'message':'\t'.join(part[4:])}
        elif part[0]=='@@DONE' and len(part)==2 and count==len(ids):
            if pending is not None:results[started]=pending
            complete=len(results)==len(ids)
            break
    if returncode!=0 or not complete:version=''
    incomplete=returncode!=0 or not version
    for case in cases:
        if case['id'] not in results:
            current=case['id']==started
            message='Süreç bu durumda sonlandı veya zaman aşımına uğradı.' if current else 'Önceki bir durum süreci sonlandırdığı için çalışmadı.'
            if not incomplete:message='Motor geçerli bir durum sonucu üretmedi.'
            results[case['id']]={'status':'SONUCSUZ','message':message}
    return results,version,exists

def execute(cases,folder,fixtures,timeout=240,engine='octave',names=()):
    """Run every case as its own function file; a parse or runtime error fails only that case.

    The runner is written in the common subset so the same files run under Octave and MATLAB.
    Returns (results, engine version, {name: exist code} for the optional names); an empty
    version marks a missing completion marker or non-zero engine exit.
    """
    folder=Path(folder);nonce=secrets.token_hex(16)
    script=[setup() if engine=='octave' else "set(0,'DefaultFigureVisible','off');\n",f"addpath({quote(fixtures)});\n"]
    (folder/'uy_kos.m').write_text(r"""function uy_kos(id,name)
  uy_marker('@@START',id);
  if exist('OCTAVE_VERSION','builtin'), fflush(stdout); end
  status='OK'; message='';
  try
    feval(name);
  catch err
    status='FAIL'; message=regexprep(err.message,'\s+',' ');
  end
  try, close('all'); catch, end
  uy_marker('@@CASE',id,status,message);
  if exist('OCTAVE_VERSION','builtin'), fflush(stdout); end
end
function uy_marker(kind,varargin)
  persistent nonce;
  if isempty(nonce), nonce=__NONCE__; end
  fprintf('%s\t%s',kind,nonce);
  for k=1:numel(varargin), fprintf('\t%s',varargin{k}); end
  fprintf('\n');
end
""".replace('__NONCE__',quote(nonce)),encoding='utf-8')
    for index,case in enumerate(cases):
        name=f'uy_durum_{index:04d}'
        (folder/f'{name}.m').write_text(f'function {name}()\n'+'\n'.join(case['code'])+'\nend\n',encoding='utf-8')
        script.append(f"uy_kos({quote(case['id'])},{quote(name)});\n")
    if names:
        (folder/'names.txt').write_text('\n'.join(names)+'\n',encoding='utf-8')
        script.append(f"uy_adlar=strsplit(strtrim(fileread('names.txt')),char(10));\nfor uy_k=1:numel(uy_adlar), fprintf('@@FN\\t%s\\t%s\\t%d\\n',{quote(nonce)},uy_adlar{{uy_k}},exist(uy_adlar{{uy_k}})); end\n")
    script.append(f"fprintf('@@VERSION\\t%s\\t%s\\n',{quote(nonce)},version());\nfprintf('@@DONE\\t%s\\n',{quote(nonce)});\n")
    (folder/'calistir.m').write_text(''.join(script),encoding='utf-8')
    output,returncode=(octave_cases_result if engine=='octave' else matlab_result)(folder/'calistir.m',folder,timeout)
    return parse_execution_output(output,cases,nonce,returncode)

def source_kind(source):
    if source.startswith(str(root/'octave'/'compat')):return 'yardımcı'
    match=re.search(r'/\.packages/([A-Za-z_-]+)-[\d.]+/',source)
    return f'paket:{match.group(1)}' if match else 'çekirdek'

def absent(name,probed):
    info=probed['functions'].get(name,{'exist':0,'source':''})
    return not name.startswith(':') and info['exist']==0 and not info['source'] and name not in probed['methods']

def blamed(case,results,probed):
    """Missing functions a failed case is attributed to; such a case says nothing about its other functions."""
    return [name for name in case['functions'] if absent(name,probed)] if results[case['id']]['status']!='OK' else []

def classify(reference,cases,results,probed):
    """Per name: yok / kısmi / doğrulandı / var (present, behaviour not exercised).

    Names starting with ':' are language features: they are not probed and only cases decide.
    """
    by_function={}
    for case in cases:
        if blamed(case,results,probed):continue
        for name in case['functions']:by_function.setdefault(name,[]).append(case['id'])
    rows={}
    for name,area in reference.items():
        ids=by_function.get(name,[]);failed=[i for i in ids if results[i]['status']!='OK']
        info=probed['functions'].get(name,{'exist':0,'source':''});owners=probed['methods'].get(name,[])
        if name.startswith(':'):kind='dil özelliği'
        elif info['exist']>0 or info['source']:kind=source_kind(info['source'])
        else:kind='yöntem:'+','.join(owners) if owners else ''
        if absent(name,probed) or (name.startswith(':') and ids and len(failed)==len(ids)):status='yok'
        elif failed or kind=='yardımcı':status='kısmi'
        elif ids:status='doğrulandı'
        else:status='var'
        rows[name]={'area':area,'status':status,'kind':kind,'cases':ids,'failed':failed}
    return rows

def area_totals(rows):
    totals={}
    for row in rows.values():
        entry=totals.setdefault(row['area'],{'toplam':0,'doğrulandı':0,'var':0,'kısmi':0,'yok':0})
        entry['toplam']+=1;entry[row['status']]+=1
    return totals

def matlab_note(cases,base=BASE):
    """How many of the current cases were confirmed by the recorded MATLAB reference run."""
    if not (base/'matlab.json').exists():return 'MATLAB referans koşusu kayıtlı değil; beklenen değerler belgelenmiş davranıştan yazıldı.'
    data=json.loads((base/'matlab.json').read_text(encoding='utf-8'))
    confirmed=len(confirmed_matlab_cases(cases,data,base/'yardimcilar'))
    return f"MATLAB {data['matlab']} referans koşusunda {confirmed} / {len(cases)} durum geçti; orada geçmeyen bir durum beklentinin yanlış olduğunu gösterir."

def write_report(path,probed,cases,results,rows):
    def reason(case):
        missing=blamed(case,results,probed)
        if missing:return 'eksik: '+', '.join(missing)
        methods=[name for name in case['functions'] if rows[name]['kind'].startswith('yöntem:')]
        return 'yalnız sınıf yöntemi: '+', '.join(methods) if methods else 'davranış farkı'
    totals=area_totals(rows);passed=sum(r['status']=='OK' for r in results.values())
    out=['# Uyumluluk envanteri','',
         f"GNU Octave {probed['version']}; yüklü paketler: {', '.join(probed['packages']) or 'yok'}. `python3 scripts/compat.py` ile üretilir; elle düzenlenmez.",'',
         'Referans adlar iki kaynaktan gelir: `uyumluluk/referans/` altındaki elle derlenmiş alan listeleri ve Octave kurulumunun kendi eksik fonksiyon listesi. Liste MATLAB fonksiyonlarının tamamı değildir.','',
         '- **Doğrulandı:** fonksiyon var ve onu kullanan tüm davranış durumları geçti.',
         '- **Var:** fonksiyon var, davranışı burada sınanmadı.',
         '- **Kısmi:** fonksiyon var, ama en az bir durum kaldı veya proje yardımcısı belgelenmiş bir alt küme sunuyor.',
         '- **Yok:** bu oturum düzeninde bulunamadı.','',
         f'Davranış durumları: {passed} / {len(cases)} geçti. {matlab_note(cases)}','',
         '| Alan | Toplam | Doğrulandı | Var | Kısmi | Yok |','|---|---:|---:|---:|---:|---:|']
    for area,t in totals.items():out.append(f"| {area} | {t['toplam']} | {t['doğrulandı']} | {t['var']} | {t['kısmi']} | {t['yok']} |")
    sums={key:sum(t[key] for t in totals.values()) for key in ('toplam','doğrulandı','var','kısmi','yok')}
    out+=[f"| **Toplam** | {sums['toplam']} | {sums['doğrulandı']} | {sums['var']} | {sums['kısmi']} | {sums['yok']} |",'','## Kalan davranış durumları','']
    failing=[case for case in cases if results[case['id']]['status']!='OK']
    if failing:
        out+=['| Durum | Alan | Neden | Not | Octave iletisi |','|---|---|---|---|---|']
        for case in failing:
            message=results[case['id']]['message'].replace('|','\\|')[:200]
            out.append(f"| `{case['id']}` | {case['area']} | {reason(case)} | {case['note']} | {message} |")
    else:out.append('Yok.')
    out+=['','## Alanlara göre eksik ve kısmi fonksiyonlar','']
    for area in totals:
        missing=sorted((n for n,r in rows.items() if r['area']==area and r['status']=='yok'),key=str.lower)
        partial=sorted((n for n,r in rows.items() if r['area']==area and r['status']=='kısmi'),key=str.lower)
        if not missing and not partial:continue
        out+=[f'### {area}','']
        if partial:out+=['Kısmi: '+', '.join(f'`{n}`' for n in partial),'']
        if missing:out+=[f'Yok ({len(missing)}): '+', '.join(f'`{n}`' for n in missing),'']
    out+=['## Geçen davranış durumları','']
    for case in cases:
        if results[case['id']]['status']=='OK':out.append(f"- `{case['id']}`: {', '.join(case['functions'])}")
    path.parent.mkdir(parents=True,exist_ok=True);path.write_text('\n'.join(out)+'\n',encoding='utf-8')

def snapshot(probed,results,rows):
    present={area:t['toplam']-t['yok'] for area,t in area_totals(rows).items()}
    return {'octave':probed['version'],'durumlar':{key:value['status'] for key,value in sorted(results.items())},'mevcut':present}

def compare(baseline,current):
    """Regressions fail the gate; improvements are only reported."""
    problems=[];gains=[]
    for key,status in current['durumlar'].items():
        before=baseline['durumlar'].get(key)
        if before is None:problems.append(f'{key}: taban çizgisinde yok (--update-baseline gerekir)')
        elif before=='OK' and status!='OK':problems.append(f'{key}: geçiyordu, şimdi {status}')
        elif before!='OK' and status=='OK':gains.append(f'{key}: artık geçiyor')
    for area,count in baseline['mevcut'].items():
        now=current['mevcut'].get(area,0)
        if now<count:problems.append(f'{area}: mevcut fonksiyon sayısı {count} → {now}')
        elif now>count:gains.append(f'{area}: mevcut fonksiyon sayısı {count} → {now}')
    return problems,gains

def run(base=BASE):
    cases=load_cases(base/'durumlar');curated=curated_reference(base/'referans')
    with tempfile.TemporaryDirectory(prefix='uyumluluk-') as folder:
        folder=Path(folder);(folder/'yokla').mkdir();(folder/'durum').mkdir()
        groups,core=octave_reference(probe([],folder/'yokla')['unimplemented'])
        reference=build_reference(curated,groups,core)
        for case in cases:
            for name in case['functions']:reference.setdefault(name,case['area'])
        probed=probe([name for name in reference if not name.startswith(':')],folder/'yokla')
        results=run_cases(cases,folder/'durum',base/'yardimcilar')
    return probed,cases,results,classify(reference,cases,results,probed)

def matlab_reference(base=BASE):
    """Run the same cases under a local MATLAB: a case that fails there has a wrong expectation."""
    if not matlab_executable():raise SystemExit('MATLAB bulunamadı (INDYMAT_MATLAB ile yol verilebilir).')
    cases=load_cases(base/'durumlar');names=[name for _,group in curated_reference(base/'referans') for name in group]
    with tempfile.TemporaryDirectory(prefix='uyumluluk-') as folder:
        results,version,exists=execute(cases,folder,base/'yardimcilar',900,'matlab',names)
    if not version:raise SystemExit('MATLAB koşusu tamamlanmadı.')
    failed={key:value['message'] for key,value in sorted(results.items()) if value['status']!='OK'}
    unknown=sorted(name for name in names if exists.get(name,0)==0)
    by_id={case['id']:case for case in cases}
    data={'schema':MATLAB_SCHEMA,'matlab':version,'durumlar':{key:matlab_record(by_id[key],value['status'],base/'yardimcilar') for key,value in sorted(results.items())},'iletiler':failed,'bulunamayan_adlar':unknown}
    if not matlab_metadata_valid(data):raise SystemExit('MATLAB sürüm bilgisi geçersiz.')
    (base/'matlab.json').write_text(json.dumps(data,ensure_ascii=False,indent=1,sort_keys=True)+'\n',encoding='utf-8')
    print(f"MATLAB {version}: {len(results)-len(failed)} / {len(results)} durum geçti")
    for key,message in failed.items():print('KALDI',key,message[:300])
    if unknown:print('MATLAB exist()==0:',' '.join(unknown))
    return 1 if failed else 0

def run_selected(prefix,engine,base=BASE):
    """Run only the cases whose id starts with the prefix; writes no report and no baseline."""
    cases=[case for case in load_cases(base/'durumlar') if case['id'].startswith(prefix)]
    if not cases:raise SystemExit(f'{prefix} ile başlayan durum yok.')
    if engine=='matlab' and not matlab_executable():raise SystemExit('MATLAB bulunamadı.')
    with tempfile.TemporaryDirectory(prefix='uyumluluk-') as folder:
        results,version,_=execute(cases,folder,base/'yardimcilar',600,engine)
    for case in cases:print(results[case['id']]['status'],case['id'],results[case['id']]['message'][:300])
    return 1 if not version or any(value['status']!='OK' for value in results.values()) else 0

def main():
    parser=argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--check',action='store_true',help='yalnız taban çizgisiyle karşılaştır, rapor yazma')
    parser.add_argument('--update-baseline',action='store_true',help='taban çizgisini bu koşunun sonucuyla değiştir')
    parser.add_argument('--matlab-reference',action='store_true',help='durumları yerel MATLAB üzerinde koş ve uyumluluk/matlab.json dosyasını yaz')
    parser.add_argument('--cases',metavar='ÖNEK',help='yalnız kimliği bu önekle başlayan durumları koş; rapor ve taban çizgisi yazılmaz')
    parser.add_argument('--engine',choices=('octave','matlab'),default='octave',help='--cases için motor')
    args=parser.parse_args()
    if args.cases:return run_selected(args.cases,args.engine)
    if args.matlab_reference:return matlab_reference()
    probed,cases,results,rows=run();current=snapshot(probed,results,rows);baseline_file=BASE/'baseline.json'
    if not args.check:write_report(REPORT,probed,cases,results,rows)
    sums={key:sum(row['status']==key for row in rows.values()) for key in ('doğrulandı','var','kısmi','yok')}
    print(f"Octave {probed['version']}: {len(rows)} referans ad; doğrulandı {sums['doğrulandı']}, var {sums['var']}, kısmi {sums['kısmi']}, yok {sums['yok']}")
    print(f"Davranış durumları: {sum(r['status']=='OK' for r in results.values())} / {len(cases)} geçti")
    if args.update_baseline:
        baseline_file.write_text(json.dumps(current,ensure_ascii=False,indent=1)+'\n',encoding='utf-8');print('Taban çizgisi güncellendi.');return 0
    if not baseline_file.exists():print('Taban çizgisi yok; --update-baseline ile oluşturun.');return 1
    problems,gains=compare(json.loads(baseline_file.read_text(encoding='utf-8')),current)
    for line in gains:print('İLERLEME',line)
    for line in problems:print('GERİLEME',line)
    if not problems:print('Taban çizgisine göre gerileme yok.')
    return 1 if problems else 0

if __name__=='__main__':sys.exit(main())
