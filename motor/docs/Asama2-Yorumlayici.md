---
title: Aşama 2 — değer modeli ve yorumlayıcı
created: 2026-10-29
type: design
status: active
---

# Aşama 2 — değer modeli ve yorumlayıcı

## Crate'ler

- `indy-value`: değer modeli, indeks aritmetiği, operatörler, birleştirme,
  sayı biçimlendirme (printf, num2str, mat2str, görüntüleme).
- `indy-interp`: ağaç yürüten yorumlayıcı, sınıflar (classdef), yerleşik
  işlevler (`builtins/*`).
- `indy-cli`: `indy run dosya.m [--path dizin]... [--cd dizin]`,
  `indy eval 'kod'`. Yakalanmayan hata: stderr'e `error: <ileti>`, çıkış 1.
  Panik: iç hata iletisi, çıkış 2; çökme dökümü yok.

## Değer modeli

- Her değer N-boyutlu, sütun öncelikli dizi; `dims` en az 2 öğe, ikinciden
  sonraki sondaki 1'ler atılır (`norm_dims`).
- `Num`: sınıf etiketi (`double`, `single`, `int8`…`uint64`), `re`, isteğe
  bağlı `im`. Sayılar her zaman f64; tamsayı ve `single` sınıfları her
  işlemden sonra yuvarlanır/doyurulur. `int64` 2^53 üstünde duyarlık kaybeder.
- `Bool`, `Char` (öğeler Unicode skaler değerleri, UTF-16 kod birimi değil),
  `Cell`, `Struct` (alan sırası korunur, öğe başına değer vektörü), `Str`
  (MATLAB string dizisi, `None` = `<missing>`), `Func` (adlı veya anonim,
  yakalanan çalışma alanıyla), `Object` (değer nesnesi: MException,
  classdef örnekleri), `HObject` (handle sınıfı: kopyalar nesneyi paylaşır).
- Hata: kimlik, char ileti, yığın, isteğe bağlı fırlatılan nesne. Her hata
  `try/catch` ile yakalanır; `catch err` bir `MException` nesnesi verir.

## Yorumlayıcı

- AST doğrudan yürütülür. Her çağrı bir `Frame` (değişkenler hash map),
  betikler çağıranın çerçevesinde çalışır. `global`/`persistent` ayrı
  depolarda, çerçeve adları bağlar.
- İndeksleme zinciri (`a.b(2).c{3}`) düzleştirilir; değişken tabanlı
  okumalar kopyasız referansla yürür, atamalar yuvada yerinde yapılır
  (`x(k) = v` döngüsü diziyi kopyalamaz).
- `end`, her argüman konumunda o anki ara değerin boyutlarına bağlanır;
  `end` aşırı yüklemesi olan nesnelerde sınıf yöntemi çağrılır.
- İşlev çözümü: yerel/iç içe işlevler → `private/` → cwd + yol (paketler
  `+pkg`) → yerleşikler → `import` listesi. Yoldaki dosya yerleşiği gölgeler
  (MATLAB/Octave kuralı). Nesne argümanı olan çağrılarda sınıf yöntemi,
  string argümanında string yöntemi önce gelir.
- Dosyalar tek tek ayrıştırılır ve önbelleğe alınır; ayrıştırılamayan dosya
  yalnız kendi çağrılarını düşürür. Geçerli MATLAB olan (ve Octave'a özgü
  blok sözcüğü içermeyen) dosyada `"..."` string skaleridir, diğer dosyalarda
  Octave kuralıyla char.
- İç içe işlevler ebeveynin çalışma alanını paylaşır (MATLAB kuralı: hem
  ebeveynde hem iç içe işlevde geçen ad paylaşılır, yalnız iç içe işlevde
  geçen ad yereldir; iç içe işlevin parametre/çıktıları yereldir). Her
  paylaşılan değişkenin tek canlı kopyası vardır: çağrıda iç içe işlevin
  çerçevesine taşınır, dönüşte geri gelir (kardeş, tutamaç, özyineleme ve
  çok düzeyli iç içelik aynı yoldan geçer). İç içe işleve tutamaç ebeveyn
  döndükten sonra da bu alanı taşır.
- `inputname(k)`: çağrı yeri, argüman yalın bir değişkense adını kaydeder
  (ad ile ya da kullanıcı işlevine tutamaçla çağrı); ifade için `''`; bir
  cs-list genişlemesinden sonraki konumlar bilinmez (`''`).
- `timer`: motorun tek iş parçacığı vardır. `start` `StartFcn`'i çalıştırır
  ve zamanlayıcıyı kuyruğa alır; vadesi gelen geri çağrılar `wait(t)` ya da
  `pause(n)` beklerken motor iş parçacığında çalışır (her ikisi en çok
  50 ms'de bir kesmeyi yoklar). Zamanlayıcı varken `pause` tam süre bekler.
  Çalışan kod bittiğinde kuyrukta kalan zamanlayıcı sonradan tetiklenmez
  (boşta döngüsü yok). `TimerFcn` hatası `ErrorFcn`'i çalıştırır ve
  zamanlayıcıyı durdurur.
- `ode45`/`ode23` adım denetimi Shampine & Reichelt, "The MATLAB ODE Suite"
  (1997) tanımını izler: varsayılan RelTol 1e-3, AbsTol 1e-6, MaxStep
  |tf−t0|/10, başlangıç adımı t0'daki eğimden, Refine 4 (ode45) / 1;
  ode45 ara noktaları Dormand–Prince sürekli genişlemesiyle (Shampine 1986)
  hesaplanır. Octave başka bir başlangıç adımı kuralı kullanır, bu yüzden
  nokta sayıları Octave'dan farklıdır.

- Hata ayıklayıcı (`src/debug.rs`): duraklama, deyim kancasının içinden
  gömücünün `DebugController`'ına çağrıdır; Rust yığını ve dolayısıyla tüm
  çerçeveler canlı kalır. Kesme noktası yokken kanca tek `bool` sınamasıdır.
  Protokol: `Kernel-Protokolu.md`, Debugger bölümü. Sınırlar: duraklamada yazılan
  kod içinde kesme noktası çalışmaz (iç içe duraklama yok); `return` yalnız o
  satırı bitirir; `dbstep N` tek adımdır; `dbstop if caught error` yok.

## Seyrek matrisler

- `Value::Sparse`: sütun sıkıştırmalı (CSC) `double`/`logical`/karmaşık, her
  zaman 2-B, açık sıfır yok, her sütunda satırlar sıralı. Yoğun yollar
  dokunulmadan kalır: işleçler ve indeksleme yalnız seyrek işlenen görünce
  `indy_value::sparse`'a gider.
- Sonuç kuralı `sparse::result_is_sparse`: iki seyrek → seyrek; karışıkta `+ -`
  ve matris işlemleri yoğun, `.*`, `&`, `S./F`, pozitif `S.^p`, karşılaştırmalar
  ve yoğun skalerle çarpma/bölme seyrek. MATLAB belgesinin açık bıraktığı
  durumlarda (`c./S`, `c.^S`, `S.^0`, `S|F`) Octave 11 davranışı izlenir.
- Yerleşikler `Callee::Builtin` kancasından (`builtins/sparse.rs::call`) geçer:
  seyreği bilenler, yalnız sınıfa/boyuta bakanlar, sıfırı koruyanlar (sıfır
  olmayanlar üzerinde çalışır), geri kalanı yoğun biçimde hesaplar.
- `A\b` ve yerel olmayan her yol yoğuna çevirir; `2^28` öğeyi aşan matris
  belleği tüketmek yerine açık "Out of memory" hatası verir. Seyrek
  çarpanlara ayırma yoktur.
- MAT dosyaları: `save`/`load` seyreği seyrek tutar (`-v7`, `-v6`: `mxSPARSE`;
  `-v4`: üçlü satırlar, logical `double` olur; `-ascii`: tam biçim). Octave 11,
  MATLAB düzenindeki logical seyreği `double` seyrek olarak okur; depolama ve
  değerler korunur, sınıf korunmaz.
- `nzmax` saklanmaz (`nzmax(S)` = `nnz(S)`); `sprand`/`sprandn` motorun `rng`
  akışını kullanır (tohumla tekrarlanır, MATLAB sayılarıyla aynı değildir);
  `rc` biçimi reddedilir; `single` seyrek yoktur.
- Görüntü düzeni (`(i,j) değer`) bellekten yazıldı; depoda MATLAB çıktısı
  referansı yok.

## Bilinen sınırlar

Nesne dizileri, `@Sınıf` klasörlerindeki yöntemler, karmaşık
doğrusal sistemler ve karmaşık matris kuvvetleri, MATLAB'ın `randn` dizisi
(Box–Muller kullanılır), sert ODE çözücüleri (`ode15s`/`ode23s` açık RK ile
çözer), grafik nesne modeli (aşama 5), Octave paketleri (`pkg load`
hata verir, aşama 4).
