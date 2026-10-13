% Ölçüm altyapısı: dalga 26 serializer doğrulaması
%% wave26-serializer-sparse-values | uy_ozet | Sparse değerler ve depolama; biçim sözleşmesi MATLAB'da yeniden doğrulanmalı
assert(strcmp(uy_ozet(sparse([1 0;0 2])),'double|2x2|<sparse>1,0,0,2,'));
assert(strcmp(uy_ozet(sparse(logical([1 0;0 1]))),'logical|2x2|<sparse>1001'));
assert(strcmp(uy_ozet(sparse(1+2i)),'double|1x1|<sparse>1+2i,'));
assert(~strcmp(uy_ozet(sparse(eye(2))),uy_ozet(eye(2))));
%% wave26-serializer-struct-arrays | uy_ozet | Alan adları ve son eleman dahil yapı dizisi değerleri
s=uy_ozet(struct('a',{1,2},'b',{3,4})); assert(isempty(strfind(s,'<partial:')));
assert(~strcmp(s,uy_ozet(struct('a',{1,9},'b',{3,4}))));
assert(strcmp(s,uy_ozet(struct('b',{3,4},'a',{1,2}))));
assert(~strcmp(uy_ozet(struct('a',{})),uy_ozet(struct('b',{}))));
%% wave26-serializer-struct-budget | uy_ozet | Yapı dizisi bütçe aşımı tam eşleşme değildir
s=uy_ozet(repmat(struct('a',1),1,10001)); assert(~isempty(strfind(s,'<partial:budget>')));
%% wave26-serializer-duration | uy_ozet seconds | Saniye ve Format sözleşmesi; nesne görüntüsünü eşitlik saymaz
x=seconds([1 NaN;Inf -2]); x.Format='s'; s=uy_ozet(x); assert(isempty(strfind(s,'<partial:')));
assert(~strcmp(s,uy_ozet(seconds([1 NaN;Inf -3]))));
y=x; y.Format='hh:mm:ss'; assert(~strcmp(s,uy_ozet(y)));
%% wave26-serializer-categorical | uy_ozet categorical | Değer kodları, kullanılmayan kategoriler ve ordinal bilgisi
x=categorical([1 NaN 2],[1 2 3],{'a','b','unused'}); s=uy_ozet(x);
assert(isempty(strfind(s,'<partial:')));
assert(~strcmp(s,uy_ozet(categorical([2 NaN 1],[1 2 3],{'a','b','unused'}))));
assert(~strcmp(s,uy_ozet(categorical([1 NaN 2],[1 2 3],{'a','b','extra'}))));
%% wave26-serializer-table | uy_ozet table | Desteklenen yerleşik değerli tabloda veri, şekil, sınıf ve metadata
x=table(int16([1;2]),[NaN 3;4 5],'VariableNames',{'a','b'}); s=uy_ozet(x);
assert(isempty(strfind(s,'<partial:')));
x.Properties.Description='changed'; assert(~strcmp(s,uy_ozet(x)));
x.Properties.UserData=@sin; assert(~isempty(strfind(uy_ozet(x),'<partial:')));
%% wave26-serializer-datetime-unzoned | uy_ozet datetime dateshift | Saat dilimsiz sonlu tarih; bileşenlerden kayıpsız kurulum şartı
x=datetime(2020,2,29,12,0,0); s=uy_ozet(x); assert(isempty(strfind(s,'<partial:')));
y=dateshift(x,'start','day'); assert(~strcmp(s,uy_ozet(y)));
x.Format='yyyy-MM-dd'; y=x; y.Format='dd-MMM-yyyy'; assert(~strcmp(uy_ozet(x),uy_ozet(y)));
