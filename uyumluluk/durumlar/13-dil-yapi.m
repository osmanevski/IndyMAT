% Dil ve programlama
%% yap-fixed-outputs | :coklu-cikti | Kontrol: sabit üç çıktı
[a,b,c]=uy_yap_sabit(2);
assert(isequal([a b c],[2 4 8]));
%% yap-runtime-counts | nargin nargout | Kontrol: çağrı içindeki girdi ve çıktı sayısı
[a,b]=uy_yap_sayilar(2,3,4);
assert(a==3 && b==2);
assert(nargin(@uy_yap_sayilar)==-1 && nargout(@uy_yap_sayilar)==2);
%% yap-varargin | :varargin | Kontrol: varargin hücre genişletmesi
assert(uy_yap_varargin(1,2,3)==6 && uy_yap_varargin(1)==1);
%% yap-ignored-input | :yoksayilan-girdi | Kontrol: girdi listesinde tilde
assert(uy_yap_yoksay(99,4)==5);
%% yap-ignored-output | :yoksayilan-cikti | Kontrol: çıktı listesinde tilde
[~,b,c]=uy_yap_sabit(2);
assert(b==4 && c==8);
%% yap-narginchk-min | :girdi-sayisi | Octave hata kimliği farklı
ok=false;
actual='hata yok';
try
uy_yap_girdi_sinir();
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:narginchk:notEnoughInputs');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:narginchk:notEnoughInputs',actual);
%% yap-narginchk-max | :girdi-sayisi | Octave hata kimliği farklı
ok=false;
actual='hata yok';
try
uy_yap_girdi_sinir(1,2,3);
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:narginchk:tooManyInputs');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:narginchk:tooManyInputs',actual);
%% yap-nargoutchk-max | :cikti-sayisi | Octave hata kimliği farklı
ok=false;
actual='hata yok';
try
[a,b]=uy_yap_cikti_sinir();
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:nargoutchk:tooManyOutputs');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:nargoutchk:tooManyOutputs',actual);
%% yap-optional-nargin | :istege-bagli-girdi | Kontrol: nargin ile isteğe bağlı girdi
assert(uy_yap_istege_bagli(3)==12 && uy_yap_istege_bagli(3,2)==6);
%% yap-early-return | :return | Kontrol: erken return
assert(uy_yap_return(-2)==0 && uy_yap_return(2)==3);
%% yap-local-handle | :yerel-fonksiyon | Kontrol: yerel işlev tutamacı dosya dışına çıkar
[y,f]=uy_yap_yerel(3);
assert(y==9 && f(4)==16);
%% yap-nested-shared | :ic-ice-fonksiyon | Kontrol: kapanışlar aynı değişkeni paylaşır
[inc,get]=uy_yap_icice(5);
assert(inc(2)==7 && get()==7);
[inc2,get2]=uy_yap_icice(1);
assert(inc2(3)==4 && get2()==4 && get()==7);
%% yap-recursive | :ozyineleme | Kontrol: dosya işlevinin özyinelemesi
assert(uy_yap_recursive(5)==120);
%% yap-private-folder | :private-klasor | Kontrol: private işlev çağıran klasörden çözülür
assert(uy_yap_private(2)==13);
%% yap-arg-class-conversion | :arguments | Octave boyutsuz sınıf bildirimini ayrıştıramaz
y=uy_yap_arg_class(single(3));
assert(isa(y,'double') && y==3);
%% yap-arg-row-reshape | :arguments | Octave boyut normalleştirmesini uygulamaz
y=uy_yap_arg_row([1;2;3]);
assert(isequal(y,[1 2 3]),'UyYap:Expectation','Beklenen [1 3], boyut %s',mat2str(size(y)));
%% yap-arg-scalar-expansion | :arguments | Octave skaler genişletmesini uygulamaz
y=uy_yap_arg_expand(4);
assert(isequal(y,4*ones(2,3)),'UyYap:Expectation','Beklenen [2 3], boyut %s',mat2str(size(y)));
%% yap-arg-dependent-default | :arguments | Octave bağımlı varsayılanı uygulamaz
assert(uy_yap_arg_default(7)==8);
%% yap-arg-numeric-rejection | :arguments | Octave doğrulama veya hata kimliği farklı
ok=false;
actual='hata yok';
try
uy_yap_arg_numeric('3');
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBeNumeric');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBeNumeric',actual);
%% yap-arg-positive-rejection | :arguments | Octave arguments doğrulamasını uygulamaz
ok=false;
actual='hata yok';
try
uy_yap_arg_positive(0);
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBePositive');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBePositive',actual);
%% yap-arg-member-rejection | :arguments | Octave arguments üyelik doğrulamasını uygulamaz
ok=false;
actual='hata yok';
try
uy_yap_arg_member('nearest');
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBeMember');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBeMember',actual);
%% yap-arg-text-rejection | :arguments | Octave arguments metin doğrulamasını uygulamaz
ok=false;
actual='hata yok';
try
uy_yap_arg_text({'a'});
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBeTextScalar');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBeTextScalar',actual);
%% yap-arg-namevalue | :arguments | Octave options alan sözdizimini ayrıştıramaz
assert(uy_yap_arg_options(3,'Scale',4,'Offset',1)==13);
%% yap-arg-name-equals | :arguments | Octave options alan sözdizimini ayrıştıramaz
assert(uy_yap_arg_options(3,Scale=4)==12);
%% yap-arg-name-no-default | :arguments | Octave varsayılansız options alanını desteklemez
assert(~uy_yap_arg_no_default() && uy_yap_arg_no_default('Label','x'));
%% yap-arg-repeating | :arguments | Octave Repeating bloğunu desteklemez
assert(uy_yap_arg_repeat(2,3,4,5)==26);
%% yap-arg-repeating-zero | :arguments | Octave sıfır tekrar girdisini desteklemez
assert(uy_yap_arg_repeat()==0);
%% yap-arg-output-conversion | :arguments | Octave Output bloğunu desteklemez
y=uy_yap_arg_output(single(5));
assert(isa(y,'double') && y==5);
%% yap-arg-output-rejection | :arguments | Octave Output doğrulamasını desteklemez
ok=false;
actual='hata yok';
try
y=uy_yap_arg_output_positive(-1);
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBePositive');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBePositive',actual);
%% yap-arg-output-repeating | :arguments | Octave Output Repeating bloğunu desteklemez
[a,b]=uy_yap_arg_output_repeat();
assert(isa(a,'double') && isa(b,'double') && a==7 && b==7);
%% yap-arg-local-validator | :arguments | Octave yerel doğrulayıcıyı çalıştırmaz
ok=false;
actual='hata yok';
try
uy_yap_arg_local_validator(2);
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'UyYap:NotOdd');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','UyYap:NotOdd',actual);
%% yap-validator-id-positive | mustBePositive | Octave doğrulayıcı hata kimliği farklı
ok=false;
actual='hata yok';
try
mustBePositive(0);
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBePositive');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBePositive',actual);
%% yap-validator-id-member | mustBeMember | Octave doğrulayıcı hata kimliği farklı
ok=false;
actual='hata yok';
try
mustBeMember('z',{'a','b'});
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBeMember');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBeMember',actual);
%% yap-validator-id-numeric | mustBeNumeric | Octave doğrulayıcı hata kimliği farklı
ok=false;
actual='hata yok';
try
mustBeNumeric('3');
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBeNumeric');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBeNumeric',actual);
%% yap-validator-id-text | mustBeText | Octave metin doğrulayıcısı yok
ok=false;
actual='hata yok';
try
mustBeText(7);
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBeText');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBeText',actual);
%% yap-validator-id-text-scalar | mustBeTextScalar | Octave metin doğrulayıcısı yok
ok=false;
actual='hata yok';
try
mustBeTextScalar({'a'});
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBeTextScalar');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBeTextScalar',actual);
%% yap-script-local-run | run | Octave betik sonundaki yerel işlevi çözemez
uy_yap_girdi=6;
run(which('uy_yap_betik'));
assert(uy_yap_sonuc==12);
%% yap-script-calls-script | :betik | Kontrol: betikler çağıranın çalışma alanını paylaşır
uy_yap_girdi=4;
uy_yap_ana_betik;
assert(uy_yap_sonuc==14);
%% yap-assignin-caller | :caller-calisma-alani | Kontrol: caller çalışma alanına yazma
uy_yap_caller_deger=2;
uy_yap_caller('set');
assert(uy_yap_caller_deger==17);
%% yap-evalin-caller | :caller-calisma-alani | Kontrol: caller çalışma alanını okuma
uy_yap_caller_deger=6;
assert(uy_yap_caller('get')==7);
%% yap-eval-output | eval | Kontrol: eval çoklu çıktı döndürür
[a,b]=eval('deal(3,4)');
assert(a==3 && b==4);
%% yap-evalc-output | evalc | Kontrol: evalc metin ve işlev çıktısını döndürür
[s,y]=evalc('uy_yap_return(4)');
assert(ischar(s) && y==5);
%% yap-inputname-expression | :girdi-adi | Kontrol: ifade girdisinin adı boştur
x=4;
assert(strcmp(uy_yap_inputname(x),'x') && isempty(uy_yap_inputname(x+1)));
%% yap-file-discovery | exist which isfile isfolder | Kontrol: dosya ve klasör sorguları
p=which('uy_yap_sabit');
assert(exist('uy_yap_sabit','file')==2 && isfile(p) && isfolder(fileparts(p)));
%% yap-mfilename | :dosya-adi | Kontrol: mfilename tam yol uzantısızdır
[n,p]=uy_yap_mfilename();
assert(strcmp(n,'uy_yap_mfilename') && isfile([p '.m']));
%% yap-dbstack-fields | dbstack | Kontrol: yığın file name line alanları
s=uy_yap_stack();
assert(isstruct(s) && isfield(s,'file') && isfield(s,'name') && isfield(s,'line') && strcmp(s(1).name,'uy_yap_stack') && s(1).line>0);
t=dbstack;
assert(isstruct(t));
%% yap-class-private-property | :classdef | Kontrol: private özellik dışarıdan okunamaz
o=uy_yap_value();
assert(o.readSecret()==9);
ok=false;
try
x=o.Secret;
catch
ok=true;
end;
assert(ok);
%% yap-class-setaccess | :classdef | Kontrol: private SetAccess dış atamayı reddeder
o=uy_yap_value();
assert(o.ReadOnly==4);
ok=false;
try
o.ReadOnly=8;
catch
ok=true;
end;
assert(ok);
%% yap-class-constant | :classdef | Kontrol: Constant sınıf üzerinden okunur
assert(uy_yap_value.Limit==12);
o=uy_yap_value();
ok=false;
try
o.Limit=1;
catch
ok=true;
end;
assert(ok);
%% yap-class-dependent | :classdef | Kontrol: Dependent get ve set yöntemleri
o=uy_yap_value(3);
assert(o.Twice==6);
o.Twice=10;
assert(o.Data==5);
%% yap-class-setter | :classdef | Kontrol: normal özelliğin set yöntemi
o=uy_yap_value();
o.Data=-7;
assert(o.Data==7);
%% yap-class-property-conversion | :classdef | Octave özellik sınıf doğrulamasını uygulamaz
o=uy_yap_validated();
o.Prop=single(4);
assert(isa(o.Prop,'double') && o.Prop==4,'UyYap:Expectation','Beklenen double, sinif %s',class(o.Prop));
%% yap-class-property-positive | :classdef | Octave özellik doğrulayıcısını uygulamaz
o=uy_yap_validated();
assert(o.Prop==1);
ok=false;
actual='hata yok';
try
o.Prop=0;
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'MATLAB:validators:mustBePositive');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','MATLAB:validators:mustBePositive',actual);
%% yap-class-method-syntax | :classdef | Kontrol: nokta ve işlev yöntem çağrısı
o=uy_yap_value(3);
assert(o.square()==9 && square(o)==9);
%% yap-class-plus | :classdef | Kontrol: plus aşırı yüklemesi
o=uy_yap_value(2)+uy_yap_value(3);
assert(o.Data==5);
%% yap-class-eq | :classdef | Kontrol: eq aşırı yüklemesi
assert(uy_yap_value(2)==uy_yap_value(2));
%% yap-class-disp | disp evalc | Kontrol: disp aşırı yüklemesi yakalanır
o=uy_yap_value(3);
f=@disp;
s=evalc('f(o)');
assert(strcmp(strtrim(s),'uy_yap_value:3'));
%% yap-class-subsref | :classdef | Kontrol: subsref parantez aşırı yüklemesi
o=uy_yap_indexed();
assert(o(2)==5);
%% yap-class-numel | numel | Kontrol: numel aşırı yüklemesi
o=uy_yap_counted();
assert(numel(o)==3);
%% yap-class-end | :classdef | Kontrol: end aşırı yüklemesi
o=uy_yap_indexed();
assert(o(end)==6);
%% yap-enum-numeric | :enumeration | Octave sayısal enum tanımını desteklemez
assert(uint8(uy_yap_enum.High)==3);
%% yap-enum-query | enumeration | Octave enum üyelerini listeleyemez
[e,n]=enumeration('uy_yap_enum');
assert(numel(e)==2 && isequal(n,{'Low';'High'}));
%% yap-class-event | addlistener notify | Octave classdef olay dinleyicisini desteklemez
o=uy_yap_events();
l=addlistener(o,'Tick',@(src,evt)src.record());
notify(o,'Tick');
assert(o.Count==1);
delete(l);
%% yap-class-superconstructor | :classdef | Kontrol: superclass kurucu çağrısı
o=uy_yap_child(7);
assert(o.Data==7 && isa(o,'uy_yap_base'));
%% yap-class-abstract | :classdef | Octave abstract yöntem bildirimi farklı
o=uy_yap_concrete();
assert(o.measure()==8 && isa(o,'uy_yap_abstract'));
%% yap-class-introspection | metaclass properties methods ismethod isprop | Kontrol: sınıf üye sorguları
o=uy_yap_value();
m=metaclass(o);
assert(strcmp(m.Name,'uy_yap_value'));
p=properties(o);
q=methods(o);
assert(any(strcmp(p,'Data')) && ~any(strcmp(p,'Secret')) && any(strcmp(q,'square')) && ismethod(o,'square') && isprop(o,'Data'));
%% yap-class-object-array | :classdef | Kontrol: nesne dizisi virgüllü özellik listesi
a(1)=uy_yap_value(2);
a(2)=uy_yap_value(5);
assert(isequal([a.Data],[2 5]));
b=a;
b(1).Data=9;
assert(a(1).Data==2);
%% yap-class-empty | :classdef | Octave classdef empty yöntemini sağlamaz
a=uy_yap_value.empty(0,2);
assert(isa(a,'uy_yap_value') && isequal(size(a),[0 2]));
%% yap-package-function | :paket-ad-alani | Kontrol: paket işlevinin tam adı
assert(uy_yap_paket.ikiKat(4)==8);
%% yap-package-import | :import | Octave import desteklemez
import uy_yap_paket.ikiKat;
assert(ikiKat(4)==8);
%% yap-package-wildcard | :import | Octave yıldızlı import desteklemez
import uy_yap_paket.*;
o=Kutu();
assert(o.Data==6 && ikiKat(3)==6);
%% yap-package-class | :paket-ad-alani | Kontrol: paket sınıfı ve static yöntem
o=uy_yap_paket.Kutu();
assert(isa(o,'uy_yap_paket.Kutu') && uy_yap_paket.Kutu.limit()==19);
%% yap-catch-exception-class | error | Octave catch bir struct döndürür
ok=false;
actual='hata yok';
try
error('UyYap:Catch','Sorun');
catch e
actual=class(e);
ok=isa(e,'MException');
end;
assert(ok,'UyYap:Expectation','Beklenen MException, sinif %s',actual);
%% yap-catch-without-variable | error | Kontrol: catch değişkensiz kullanılabilir
ok=false;
try
error('Sorun');
catch
ok=true;
end;
assert(ok);
%% yap-exception-stack-empty | MException | Octave MException nesnesi yok
e=MException('UyYap:Stack','Sorun');
assert(isempty(e.stack) && isstruct(e.stack) && isfield(e.stack,'file') && isfield(e.stack,'name') && isfield(e.stack,'line'));
%% yap-exception-cause | MException addCause | Octave neden zinciri yok
a=MException('UyYap:Outer','Dis');
b=MException('UyYap:Inner','Ic');
a=addCause(a,b);
assert(iscell(a.cause) && numel(a.cause)==1 && strcmp(a.cause{1}.identifier,'UyYap:Inner'));
%% yap-rethrow-stack | :hata-yigini | Kontrol: rethrow ilk hata yığınını korur
ok=false;
try
uy_yap_rethrow();
catch e
ok=~isempty(strfind(e.stack(1).name,'uy_yap_error_leaf'));
end;
assert(ok);
%% yap-throw-as-caller | :hata-yigini | Octave MException oluşturamaz
ok=false;
actual='hata yok';
try
uy_yap_throw_caller();
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'UyYap:Caller') && ~any(strcmp({e.stack.name},'uy_yap_throw_caller'));
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','UyYap:Caller',actual);
%% yap-report-causes | getReport MException addCause | Octave MException neden raporu yok
a=MException('UyYap:Outer','outer-text');
a=addCause(a,MException('UyYap:Inner','inner-text'));
s=getReport(a,'extended','hyperlinks','off');
assert(~isempty(strfind(s,'outer-text')) && ~isempty(strfind(s,'inner-text')));
%% yap-warning-state | warning | Kontrol: kimliğe göre warning durumu struct
old=warning('query','UyYap:Warning');
c=onCleanup(@()warning(old));
warning('off','UyYap:Warning');
s=warning('query','UyYap:Warning');
assert(isstruct(s) && strcmp(s.identifier,'UyYap:Warning') && strcmp(s.state,'off'));
%% yap-warning-identifier | warning lastwarn evalc | Kontrol: warning biçim argümanı ve kimliği
old=warning('query','UyYap:Warning');
c=onCleanup(@()warning(old));
warning('on','UyYap:Warning');
[oldmsg,oldid]=lastwarn;
d=onCleanup(@()lastwarn(oldmsg,oldid));
s=evalc('warning(''UyYap:Warning'',''Deger %d'',7)');
[msg,id]=lastwarn;
assert(strcmp(msg,'Deger 7') && strcmp(id,'UyYap:Warning') && ~isempty(s));
%% yap-cleanup-exception | onCleanup | Kontrol: onCleanup hata ile çıkışta çalışır
p=[tempname '.txt'];
fid=fopen(p,'w');
assert(fid~=-1);
fclose(fid);
c=onCleanup(@()uy_yap_delete_if_exists(p));
ok=false;
try
uy_yap_cleanup_throw(p);
catch e
ok=strcmp(e.identifier,'UyYap:Cleanup');
end;
assert(ok && ~isfile(p));
clear c
%% yap-assert-identifier | assert | Kontrol: assert kimlik ve biçim mesajı
ok=false;
actual='hata yok';
try
assert(false,'UyYap:Assert','Deger %d',7);
catch e
actual=[e.identifier ': ' e.message];
ok=strcmp(e.identifier,'UyYap:Assert') && strcmp(e.message,'Deger 7');
end;
assert(ok,'UyYap:Expectation','Beklenen %s, alinan %s','UyYap:Assert',actual);
%% yap-validateattributes-shape | validateattributes | Kontrol: ncols ve nrows doğrulaması
validateattributes(ones(2,3),{'double'},{'nrows',2,'ncols',3});
ok=false;
try
validateattributes(ones(3,2),{'double'},{'ncols',3});
catch
ok=true;
end;
assert(ok);
%% yap-parser-optional | inputParser addOptional parse | Kontrol: addOptional varsayılanı
p=inputParser;
addOptional(p,'Count',4,@isnumeric);
parse(p);
assert(p.Results.Count==4);
parse(p,7);
assert(p.Results.Count==7);
%% yap-parser-unmatched | inputParser addParameter parse | Kontrol: KeepUnmatched bilinmeyen çiftleri saklar
p=inputParser;
p.KeepUnmatched=true;
addParameter(p,'Scale',1);
parse(p,'Other',7);
assert(p.Unmatched.Other==7 && p.Results.Scale==1);
%% yap-parser-defaults | inputParser addParameter parse | Kontrol: UsingDefaults yalnız kullanılmayan adlar
p=inputParser;
addParameter(p,'Scale',1);
addParameter(p,'Offset',0);
parse(p,'Scale',2);
assert(isequal(p.UsingDefaults,{'Offset'}));
%% yap-parser-structexpand | inputParser addParameter parse | Kontrol: StructExpand parametre yapısını açar
p=inputParser;
addParameter(p,'Scale',1);
parse(p,struct('Scale',3));
assert(p.Results.Scale==3);
%% yap-parser-partial | inputParser addParameter parse | Kontrol: parametre kısmi eşleştirmesi
p=inputParser;
p.PartialMatching=true;
addParameter(p,'Scale',1);
parse(p,'Sca',3);
assert(p.Results.Scale==3);
