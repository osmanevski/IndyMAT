"""Bounded, read-only symbol indexing for the current folder."""
from __future__ import annotations
from backend.i18n import tr

import os
import json
import re
import stat
import time
from pathlib import Path


IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
FUNCTION = re.compile(r"^\s*function(?:\s+(?:\[[^\]]*\]|[A-Za-z_]\w*)\s*=\s*|\s+)([A-Za-z_]\w*)\s*(?:\(|$)", re.I)
MAX_FUNCTIONS = 2000
MAX_RESULT_BYTES = 262_144


def _json_bytes(value):
    # Match Handler.send, including UTF-8, escaping and default separators.
    return len(json.dumps(value,ensure_ascii=False).encode('utf-8'))


class _ResultBudget:
    def __init__(self,remaining):
        self.remaining=remaining

    def append(self,records,record):
        # Charge the comma/space even for the first item: conservative by two
        # bytes per nonempty array, with no growing-list serialization.
        size=_json_bytes(record)+2
        if size>self.remaining:return False
        self.remaining-=size
        records.append(record)
        return True


def _starts_string(line, index):
    previous=index-1
    while previous>=0 and line[previous].isspace():previous-=1
    return previous<0 or line[previous] in '=([{,;:+-*/\\^~<>|&'


def lexical_lines(source,deadline=None):
    """Return source-aligned code with strings and comments blanked."""
    blocks=0;quote=''
    for number,line in enumerate(source.splitlines(),1):
        if deadline is not None and time.monotonic()>deadline:return
        trimmed=line.lstrip()
        if not quote and re.fullmatch(r'[%#]\{\s*',trimmed):
            blocks+=1;yield number,'';continue
        if blocks:
            if re.fullmatch(r'[%#]\}\s*',trimmed):blocks-=1
            yield number,'';continue
        code=[];index=0
        while index<len(line):
            if deadline is not None and index%4096==0 and time.monotonic()>deadline:return
            character=line[index]
            if quote:
                code.append(' ')
                if character=='\\' and quote=='"' and index+1<len(line):
                    code.append(' ');index+=1
                elif character==quote:
                    if index+1<len(line) and line[index+1]==quote:
                        code.append(' ');index+=1
                    else:quote=''
            elif character in ('%','#') or line[index:index+3]=='...':
                code.extend(' '*(len(line)-index));break
            elif character=='"' or character=="'" and _starts_string(line,index):
                quote=character;code.append(' ')
            else:code.append(character)
            index+=1
        yield number,''.join(code)


def parse_source(source,path,name=None,occurrence_limit=2000,deadline=None,*,function_limit=MAX_FUNCTIONS,result_byte_limit=MAX_RESULT_BYTES,budget=None):
    functions=[];occurrences=[];truncated=False
    if budget is None:
        budget=_ResultBudget(result_byte_limit-_json_bytes({'functions':[],'occurrences':[],'truncated':False}))
    for number,code in lexical_lines(source,deadline):
        match=FUNCTION.match(code)
        if match:
            if len(functions)>=function_limit or not budget.append(functions,{'name':match.group(1),'path':str(path),'line':number}):
                return functions,occurrences,True
        if name is not None:
            for token in IDENTIFIER.finditer(code):
                if token.group(0)!=name:continue
                if len(occurrences)>=occurrence_limit or not budget.append(occurrences,{'name':name,'path':str(path),'line':number,'column':token.start()+1}):
                    return functions,occurrences,True
    return functions,occurrences,truncated or deadline is not None and time.monotonic()>deadline


class SymbolIndex:
    MAX_ENTRIES=2000
    MAX_FILES=200
    MAX_FILE_BYTES=500_000
    MAX_TOTAL_BYTES=2_000_000
    MAX_OCCURRENCES=2000
    MAX_FUNCTIONS=MAX_FUNCTIONS
    MAX_RESULT_BYTES=MAX_RESULT_BYTES
    TIME_LIMIT=.5

    @staticmethod
    def _open_folder(folder,roots):
        folder=Path(folder)
        if not folder.is_absolute() or not any(folder.is_relative_to(Path(root)) for root in roots):
            raise PermissionError(tr('The symbol folder is outside the allowed area.'))
        descriptor=os.open('/',os.O_RDONLY|os.O_DIRECTORY)
        try:
            for part in folder.parts[1:]:
                next_descriptor=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=descriptor)
                os.close(descriptor);descriptor=next_descriptor
            return descriptor
        except OSError as exc:
            os.close(descriptor);raise PermissionError(tr('The symbol folder contains a link or an inaccessible path.')) from exc

    def scan(self,folder,roots,name=None):
        if name is not None and (not isinstance(name,str) or not IDENTIFIER.fullmatch(name)):
            raise ValueError(tr('Select a valid function or variable name.'))
        started=time.monotonic();deadline=started+self.TIME_LIMIT
        # Reserve the full response envelope at maximum counter widths before
        # parsing. One shared budget covers BOTH arrays across ALL files.
        result={'folder':str(folder),'functions':[],'occurrences':[],'truncated':False,
                'limits':{'entries':self.MAX_ENTRIES,'files':self.MAX_FILES,'file_bytes':self.MAX_FILE_BYTES,
                          'total_bytes':self.MAX_TOTAL_BYTES,'occurrences':self.MAX_OCCURRENCES,
                          'functions':self.MAX_FUNCTIONS,'result_bytes':self.MAX_RESULT_BYTES,'time_ms':round(self.TIME_LIMIT*1000)},
                'scanned_files':self.MAX_FILES,'scanned_bytes':self.MAX_TOTAL_BYTES}
        budget=_ResultBudget(self.MAX_RESULT_BYTES-_json_bytes(result))
        if budget.remaining<0:raise ValueError(tr('The symbol response exceeds the size limit.'))
        descriptor=self._open_folder(folder,roots)
        functions=[];occurrences=[];files=0;total=0;entries=0;truncated=False
        try:
            candidates=[]
            with os.scandir(descriptor) as listing:
                for entry in listing:
                    entries+=1
                    if entries>self.MAX_ENTRIES or time.monotonic()>deadline:
                        truncated=True;break
                    if entry.name.startswith('.') or not entry.name.lower().endswith('.m'):continue
                    try:metadata=entry.stat(follow_symlinks=False)
                    except OSError:
                        truncated=True;continue
                    if stat.S_ISREG(metadata.st_mode):candidates.append((entry.name,metadata))
            for filename,metadata in sorted(candidates,key=lambda item:item[0].casefold()):
                if files>=self.MAX_FILES or time.monotonic()>deadline:
                    truncated=True;break
                if metadata.st_size>self.MAX_FILE_BYTES:
                    truncated=True;continue
                if total+metadata.st_size>self.MAX_TOTAL_BYTES:
                    truncated=True;break
                try:file_descriptor=os.open(filename,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=descriptor)
                except OSError:
                    truncated=True;continue
                try:
                    opened=os.fstat(file_descriptor)
                    if not stat.S_ISREG(opened.st_mode) or (opened.st_dev,opened.st_ino)!=(metadata.st_dev,metadata.st_ino):
                        truncated=True;continue
                    chunks=[];remaining=self.MAX_FILE_BYTES+1
                    while remaining and time.monotonic()<=deadline:
                        chunk=os.read(file_descriptor,min(65536,remaining))
                        if not chunk:break
                        chunks.append(chunk);remaining-=len(chunk)
                    data=b''.join(chunks);after=os.fstat(file_descriptor)
                    if len(data)>self.MAX_FILE_BYTES or (opened.st_size,opened.st_mtime_ns,opened.st_ctime_ns)!=(after.st_size,after.st_mtime_ns,after.st_ctime_ns):
                        truncated=True;continue
                    if len(data)!=opened.st_size:
                        truncated=True;break
                finally:os.close(file_descriptor)
                if total+len(data)>self.MAX_TOTAL_BYTES:
                    truncated=True;break
                try:source=data.decode('utf-8-sig')
                except UnicodeDecodeError:
                    truncated=True;continue
                files+=1;total+=len(data)
                remaining_occurrences=max(0,self.MAX_OCCURRENCES-len(occurrences))
                remaining_functions=max(0,self.MAX_FUNCTIONS-len(functions))
                found,uses,limited=parse_source(source,Path(folder)/filename,name,remaining_occurrences,deadline,
                                                function_limit=remaining_functions,budget=budget)
                functions.extend(found);occurrences.extend(uses);truncated=truncated or limited
                if limited or len(functions)>=self.MAX_FUNCTIONS or time.monotonic()>deadline or name is not None and len(occurrences)>=self.MAX_OCCURRENCES:
                    truncated=True;break
        finally:os.close(descriptor)
        result.update(functions=functions,occurrences=occurrences,truncated=truncated,scanned_files=files,scanned_bytes=total)
        return result
