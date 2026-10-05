import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class TextHelperTests(unittest.TestCase):
    def run_octave(self, body):
        command = ("warning('off','Octave:shadowed-function'); "
                   "original=struct(); for name={'blanks','native2unicode','strtrim'}, "
                   "original.(name{1})=str2func(name{1}); endfor; "
                   "addpath('%s'); addpath('%s'); %s") % (
                       ROOT / 'octave' / 'compat',
                       ROOT / '.packages' / 'datatypes-1.5.0', body)
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--eval', command],
                                cwd=ROOT, text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_original_forms_and_resolution_do_not_change_path(self):
        self.run_octave("p=path(); folder=pwd(); "
                        "for value={1,5,int32(3)}, assert(isequal(blanks(value{1}),original.blanks(value{1}))); endfor; "
                        "for value={' a ',{' a ',' b '},char(' a ','b  ')}, assert(isequal(strtrim(value{1}),original.strtrim(value{1}))); endfor; "
                        "for value={uint8([195 169]),uint8([97;98]),'abc'}, assert(isequal(native2unicode(value{1},'UTF-8'),original.native2unicode(value{1},'UTF-8'))); endfor; "
                        "assert(strcmp(path(),p) && strcmp(pwd(),folder)); "
                        "for name={'blanks','native2unicode','strtrim'}, "
                        "args={-1}; if strcmp(name{1},'native2unicode'), args={[1 2;3 4]}; elseif strcmp(name{1},'strtrim'), args={12}; endif; "
                        "try, original.(name{1})(args{:}); catch a, end; try, feval(name{1},args{:}); catch b, end; "
                        "assert(strcmp(a.identifier,b.identifier) && strcmp(a.message,b.message)); endfor")

    def test_empty_byte_vectors_and_blank_row_shape(self):
        self.run_octave("for value={0,int32(0),false}, assert(isequal(size(blanks(value{1})),[1 0])); endfor; "
                        "for value={uint8([]),zeros(1,0),zeros(0,1)}, assert(isequal(size(native2unicode(value{1},'UTF-8')),[0 0])); endfor; "
                        "failed=false; try, native2unicode(uint8([]),'invalid-codepage'); catch, failed=true; end; assert(failed);")

    def test_string_trim_keeps_missing_elements_and_shape(self):
        self.run_octave("s=string({' a ',' b ';' c ',' d '}); s(2,1)=missing; "
                        "r=strtrim(s); assert(isa(r,'string') && isequal(size(r),[2 2])); "
                        "assert(ismissing(r(2,1)) && strcmp(char(r(1,1)),'a') && strcmp(char(r(2,2)),'d')); "
                        "assert(strcmp(char(strtrim(string(sprintf('\\tabc\\r\\n')))),'abc'));")

    def test_regexp_selectors_and_pass_through(self):
        self.run_octave("assert(isequal(size(regexp('abc','x')),[0 0])); "
                        "assert(isequal(size(regexp('abc','x','match')),[0 0])); "
                        "for options={{},{'match'},{'tokens'},{'names'},{'tokenExtents'},{'once','match'},{'start','end'}}, "
                        "[a,b]=regexp('x12 y34','(?<key>[a-z]+)([0-9]+)',options{1}{:}); "
                        "[c,d]=builtin('regexp','x12 y34','(?<key>[a-z]+)([0-9]+)',options{1}{:}); "
                        "assert(isequal(a,c) && isequal(b,d)); endfor; "
                        "assert(isequal(regexp({'abc','x'},'x'),builtin('regexp',{'abc','x'},'x'))); "
                        "regexp('abc','a');")

    def test_json_pretty_preserves_escaped_strings_and_structure(self):
        self.run_octave("x=struct('text',['a,{}[]:' char(34) char(92) sprintf('\\n') 'ç'], "
                        "'values',{{[],struct('nested',[true false]),[NaN Inf 2]}}); "
                        "compact=builtin('jsonencode',x); pretty=jsonencode(x,'PrettyPrint',true); "
                        "assert(~isempty(strfind(pretty,sprintf('\\n')))); "
                        "assert(isequaln(jsondecode(pretty),jsondecode(compact))); "
                        "assert(strcmp(jsonencode(x),compact)); "
                        "assert(strcmp(jsonencode(x,'PrettyPrint',false),builtin('jsonencode',x,'PrettyPrint',false))); "
                        "assert(strcmp(jsonencode(struct(),'PrettyPrint',true),'{}')); "
                        "assert(strcmp(jsonencode([],'PrettyPrint',true),'[]')); "
                        "assert(isequaln(jsondecode(jsonencode([NaN Inf],'ConvertInfAndNaN',false,'PrettyPrint',true)),[NaN;Inf]));")

    def test_insert_positions_boundaries_shapes_and_unicode(self):
        self.run_octave("assert(strcmp(insertBefore('abc',1,'!'),'!abc')); "
                        "assert(strcmp(insertBefore('abc',4,'!'),'abc!')); "
                        "assert(strcmp(insertAfter('abc',0,'!'),'!abc')); "
                        "assert(strcmp(insertAfter('abc',3,'!'),'abc!')); "
                        "assert(strcmp(insertBefore('aaaa','aa','!'),'!aa!aa')); "
                        "assert(strcmp(insertAfter('aaaa','aa','!'),'aa!aa!')); "
                        "assert(strcmp(insertBefore('abc','x','!'),'abc')); "
                        "assert(strcmp(insertBefore('çığ',2,'!'),'ç!ığ')); "
                        "assert(strcmp(insertAfter('çığ','ı','!'),'çı!ğ')); "
                        "assert(isequal(insertBefore({'ab';'cd'},[1;3],{'X';'Y'}),{'Xab';'cdY'})); "
                        "assert(isequal(insertAfter({'ab','cd'},1,'!'),{'a!b','c!d'})); "
                        "assert(strcmp(insertBefore('',1,'x'),'x') && strcmp(insertAfter('',0,'x'),'x')); "
                        "assert(isa(insertBefore(string('abc'),2,'!'),'string')); "
                        "for args={{'abc',5,'!'},{'abc',1.5,'!'},{'abc','','!'},{'😀',1,'!'},{'abc',1,42}}, "
                        "failed=false; try, insertBefore(args{1}{:}); catch e, failed=~isempty(strfind(e.message,'insertBefore:')); end; assert(failed); endfor;")

    def test_extract_between_scalar_multiple_matches_are_columns(self):
        self.run_octave("assert(isequal(extractBetween('[a][b]','[',']'),{'a';'b'})); "
                        "assert(isequal(extractBetween('[a][b]','[',']','Boundaries','inclusive'),{'[a]';'[b]'})); "
                        "assert(isequal(extractBetween({'abc','def'},1,2),{'ab','de'}));")

    def test_textscan_character_fields_skip_and_collection(self):
        self.run_octave("r=textscan('12345 abcdef','%3d%4c'); assert(isequal(r,{int32(123),'45 a'})); "
                        "r=textscan('ab cd ef gh','%2c%2c','CollectOutput',true); "
                        "assert(isequal(r,{char('abcd','efgh')})); "
                        "r=textscan('ab cd','%2c%2s','CollectOutput',true); assert(isequal(r,{'ab',{'cd'}})); "
                        "r=textscan('12 a 34 b','%d%c','CollectOutput',true); assert(isequal(r,{int32([12;34]),char('a','b')})); "
                        "for args={{'1 2 3','%f%*f%f'},{'1,,3','%f%f%f','Delimiter',','},{'ab 12','%*2c%f'},{'1 a','%f%s'}}, "
                        "assert(isequaln(textscan(args{1}{:}),builtin('textscan',args{1}{:}))); endfor;")

    def test_unchanged_call_overhead_is_bounded(self):
        result = self.run_octave("names={'blanks','strtrim','native2unicode','regexp','jsonencode','textscan'}; "
                                 "args={{3},{' a '},{uint8([97 98]),'UTF-8'},{'abc','a'},{struct('a',1)},{'1 2','%f'}}; "
                                 "n=4000; for k=1:numel(names), name=names{k}; a=args{k}; "
                                 "fn=str2func(name); value=fn(a{:}); if isfield(original,name), base=original.(name); "
                                 "else, base=@(varargin)builtin(name,varargin{:}); endif; "
                                 "deltas=[]; for trial=1:3, tic; for j=1:n, value=base(a{:}); endfor; b=toc; "
                                 "tic; for j=1:n, value=fn(a{:}); endfor; w=toc; deltas(end+1)=(w-b)/n; endfor; "
                                 "overhead=median(deltas); fprintf('%s %.3f us\\n',name,overhead*1e6); assert(overhead<150e-6); endfor;")
        print(result.stdout.strip())
