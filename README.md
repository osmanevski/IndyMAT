# IndyMAT

<picture><source media="(prefers-color-scheme: dark)" srcset="docs/brand/logo/svg/indymat-logo-horizontal-white.svg"><img src="docs/brand/logo/svg/indymat-logo-horizontal-black.svg" alt="IndyMAT" width="280"></picture>

**GNU Octave diliyle bilimsel hesaplama, veri analizi ve grafik üretimi için Türkçe ve İngilizce arayüzlü bir masaüstü çalışma ortamı.** Arayüz ilk açılışta sistem dilini izler; dil Ayarlar'dan değiştirilebilir. IndyMAT; kod editörünü, komut penceresini, değişken incelemesini ve grafikleri aynı pencerede bir araya getirir. Hesaplamalar bilgisayarınızda çalışır; normal kullanım için bulut hesabı veya internet bağlantısı gerekmez.

*English:* IndyMAT is a local desktop workspace for scientific computing with GNU Octave: editor, Command Window, Workspace and figures in one window. An optional Assistant panel can drive coding-agent command line programs you already have installed (Claude Code, Codex CLI, Antigravity CLI), with approval cards for changes and optional access to the live session; IndyMAT itself makes no model calls. The interface is available in English and Turkish, follows the system language on first start, and uses the terms MATLAB users already know (Workspace, Command Window, Run Section). The rest of this document is in Turkish.

## Hangi dil ile çalışır?

IndyMAT'te **GNU Octave dilinde** komutlar, `.m` betikleri ve fonksiyonlar yazılır. Octave, matris ve vektör işlemleri üzerine kurulu sayısal hesaplama dilidir. Hesaplamaları gerçek GNU Octave motoru yürütür; değişkenler komutlar arasında bellekte korunur.

Uygulamanın teknik yapısı:

| Bileşen | Teknoloji |
| --- | --- |
| Sayısal hesaplama ve betikler | GNU Octave |
| Yerel sunucu ve süreç yönetimi | Python standart kütüphanesi |
| Editör ve arayüz | JavaScript, CodeMirror 6, HTML ve CSS |
| macOS uygulama penceresi | Swift, AppKit ve WebKit |

## Ne işe yarar?

- Matris işlemleri, doğrusal cebir ve kompleks sayılarla hesaplama.
- Veri analizi, interpolasyon, FFT ve diferansiyel denklem çözümleri.
- Sinyal işleme ve filtre tasarımı; kontrol sistemleri ve frekans cevabı inceleme.
- 2B ve 3B grafik oluşturma; desteklenen grafiklerde yakınlaştırma, kaydırma, 3B döndürme ve veri ipuçları.
- Blok diyagramı modellerini metin olarak (netlist) tanımlama ve Octave üzerinde simüle etme: `im_compile` modeli doğrular, `im_simulate` çalıştırır, `im_example_model` hazır örnek modeller kurar. Model biçimi `octave/block-models.md` dosyasında anlatılır; `workspace/examples/netlist_*.m` örnekleri hazır gelir.
- Blok diyagramı editörü: Geçerli klasörde açılan `.imbd.json` dosyası diyagram olarak gösterilir. `im_compile`'ın 13 blok türü eklenir, taşınır, bağlanır ve silinir. Parametreler yan panelde yalnız sayı olarak girilir. Geri alma, klavyeyle düzenleme ve metin olarak açma vardır. Çalıştır, modeli görünür bir komutla `im_simulate`'e verir ve her Scope'u ayrı bir grafikte çizer. Editör IndyMAT blok modellerini düzenler; Simulink uyumluluğu değildir. Örnek: `workspace/examples/pid_kapali_cevrim.imbd.json`.
- Ders örnekleri, mühendislik hesapları ve tekrarlanabilir sayısal deneyler hazırlama.

## Çalışma ortamı

- Sekmeli editör, sözdizimi renklendirme, kod katlama, tamamlama ve arama/değiştirme; anahat, tanıma gitme ve kullanımları bulma.
- Dosya, seçili kod veya `%%` bölümü çalıştırma, bölümü çalıştırıp ilerleme; komut geçmişi paneli ve hesabı durdurma.
- Ev dizini içinde dosya gezintisi; yeni dosya ve klasör, yeniden adlandırma, kopyalama, taşıma ve çöp kutusuna gönderme; kaydetme çakışması denetimi ve yerel taslaklar.
- Çalışma alanında değişkenlerin değer, tür ve boyutlarını inceleme; yeniden adlandırma, silme, MAT dosyasına kaydetme ve yükleme; değişken düzenleyicide hücre düzenleme.
- Octave sözdizimi denetimi; koşullu kesme noktaları, çağrı yığını, imlece kadar çalıştırma ve adımlı hata ayıklama.
- Çalıştırma süresi ve fonksiyon profili; betikten HTML rapor üretme.
- Fonksiyon yardımı, paket yönetimi, açık ve koyu tema, boyutlanabilir paneller ve değiştirilebilir klavye kısayolları.
- Türkçe ve İngilizce arayüz; Türkçe arayüzde çalıştırma ve hata ayıklama düğmelerinin ipucunda MATLAB'daki adı da görünür.

Ayarlar'daki deneysel metin uyarlaması varsayılan olarak kapalıdır. Editör seçimleri/bölümleri ve kayıtlı giriş betikleri için ayrı ayrı açılabilir. Desteklenen çift tırnaklı metin ifadeleri çalıştırma sırasında uyarlanır; kayıtlı `.m` dosyası değiştirilmez. Belirsiz sözdizimi, yerel fonksiyonlar, özel/paket dizinleri veya etkin hata ayıklama/profil durumu varsa dosyanın tamamı değiştirilmeden çalışır. Bu seçenek tam dil ya da Toolbox uyumluluğu sağlamaz; uyarlanan giriş betiğinde `mfilename` ve ham çağrı yığını geçici yürütme dosyasını gösterir.

`control`, `signal`, `datatypes` ve `statistics` paketleri kuruluysa oturum başlarken yüklenir. Paketler depoya dahil değildir; ayrı kurulurlar. Uygulamanın paket yöneticisi onları projenin `.packages/` klasörüne kurar.

Bu paketler için sürümlü yamalar `octave/paket-yamalari/` altındadır. Her yamanın değiştirdiği dosyaların tam listesi kendi klasöründeki `manifest.json` dosyasındadır:

- `datatypes`: string sınıfındaki bazı karışık char/string çağrıları.
- `control`: sürekli zamanlı tek giriş-tek çıkışlı `zpk` ve `pid` gösterimleri. Bunlarla birlikte `bode`, `nyquist`, `margin`, `minreal`, `c2d`/`d2c`, `tf` özelliklerini okuma/yazma ve sistemlerin toplama, çıkarma, çarpma, bölme, `feedback`, `series`, `parallel` işlemleri de değişir.
- `signal`: `sos2tf`, `interp`, `fir1`, `fir2`, `pwelch`, `findpeaks`, `resample`, `hann`, `ellip`, `buttord`, `cheb2ord`, `besself`, `tf2ss` ve `residuez`.
- `statistics`: `norminv`, `poisspdf` ve `grpstats`.

Bu fonksiyonların sonuçları yamasız paketinkinden farklı olabilir. `python3 scripts/patch_packages.py --package <paket-sürüm>` (örneğin `control-4.2.3`) yamaları projenin `.packages/` klasöründeki pakete uygular; sistem genelinde kurulu paketlere dokunmaz. `--package` verilmezse yalnızca `datatypes` işlenir. `--check` yalnızca durumu gösterir, `--revert` özgün dosyaları geri yükler. Betik paketin kendi dosyalarını yerinde değiştirir, ama yalnızca içerikleri beklenen özgün sürümle birebir aynıysa. Tanımadığı bir paket sürümüne, içeriği değişmiş bir dosyaya ve oluşturacağı yerde zaten var olan bir dosyaya dokunmaz. Bir paket yeniden kurulur ya da güncellenirse yamalar kaybolur; betiği o paket için yeniden çalıştırın.

## Kodlama asistanları (isteğe bağlı)

Sağdaki Asistan paneli, bilgisayarınızda zaten kurulu ve oturum açılmış kodlama asistanı komut satırı programlarını (Claude Code, Codex CLI, Antigravity CLI) geçerli klasörde çalıştırır. IndyMAT'ın kendisi hiçbir model çağrısı yapmaz, anahtar ya da model barındırmaz; program kurulu değilse o seçenek kapalı görünür ve uygulamanın geri kalanı aynen çalışır.

- Asistan, editörde açık olan dosyayı ve geçerli klasörü kendiliğinden bilir; motorun GNU Octave olduğu ve yüklü paketler de ona bildirilir.
- Dosya erişimi görüşme başına seçilir: **Salt okunur**, **Değişiklikten önce sor** (Claude Code ve Codex için varsayılan) ve **Geçerli klasörde düzenle**. "Sor" kipinde asistanın izin gerektiren her işlemi panelde bir onay kartı olur; dosya düzenlemeleri fark önizlemesiyle gösterilir.
- Oturum erişimi ayrıca seçilir: **Yok** (varsayılan), **Değişkenleri ve grafikleri gör**, **Oturumumda kod çalıştır**. Çalıştırılan kod Komut penceresinde görünür ve sıradan bir iş gibi durdurulabilir. Bu erişim Claude Code ve Codex ile kullanılabilir.
- Kaydedilmemiş taslaklar asistanın disk üzerindeki değişiklikleriyle ezilmez; yalnızca "diskte değişti" diye işaretlenir.

Asistanın çalıştırdığı kod ve komutlar sizin kullanıcı yetkilerinizle çalışır; "Oturumumda kod çalıştır" ve "Geçerli klasörde düzenle" seçeneklerini buna göre kullanın. Asistanlara gönderilen metin, ilgili programın kendi hesabı ve koşullarıyla sağlayıcısına gider.

## Başlatma

Python **3.10+** ve GNU Octave gerekir. macOS üzerinde Python 3.11 ve Octave 11.3.0 ile doğrulanmıştır. Arayüz hazır derlenmiş olarak gelir; normal kullanımda Node.js gerekmez.

```sh
git clone https://github.com/osmanevski/IndyMAT.git
cd IndyMAT
python3 app.py
```

Mac'te `start.command` dosyasına çift tıklayarak da başlatabilirsiniz. Başlatıcının açtığı oturum bağlantısını kullanın. Sunucu yalnızca yerel bilgisayarda, varsayılan olarak `127.0.0.1:8769` adresinde dinler.

Kendi macOS uygulama penceresini oluşturmak için Xcode Command Line Tools kurulu olmalıdır:

```sh
python3 macos/build_app.py
python3 macos/install_app.py
```

Ardından Uygulamalar klasöründen **IndyMAT** açılabilir. Uygulama mevcut proje klasörünü ve Python/Octave kurulumunu kullanır. Pencereyi kapatmak hesaplama oturumunu sonlandırmaz.

## İlk hesaplama

Örnekler menüsünden bir dosya açın veya komut penceresine yazın:

```octave
A = [3, 1; 1, 2];
b = [9; 8];
x = A \ b

t = linspace(0, 2*pi, 200);
plot(t, sin(t));
xlabel('t');
ylabel('sin(t)');
grid on;
```

**F5** dosyayı kaydedip çalıştırır, **F9** seçimi, **⌘Enter / Ctrl+Enter** mevcut bölümü çalıştırır. Değişkenleri kalıcı saklamak için `save('veriler.mat')`, tekrar yüklemek için `load('veriler.mat')` kullanın.

## Kapsam ve sınırlar

- macOS doğrulanmıştır; Linux kabul testleri yapılmamıştır, Windows başlatıcısı desteklenmez.
- Grafikler hesaplama sonunda aktarılır. Etkileşimli görünüm belgelenmiş bir alt kümedir:
  - 2B: `plot`, `stairs`, `stem`, `scatter`, `bar`, `area`, `errorbar`, `xline`/`yline`, `image`/`imagesc`, `fill`/`patch`, `text`, `contour` ve `contourf`.
  - 3B: `plot3`, `scatter3`, `surf`, `mesh`, `contour`/`contourf` ve `patch`.
  - Tek değerli saydamlık etkileşimlidir: `surf`, `mesh`, `patch`, `fill`, `area`, `bar` ve `contourf` için `FaceAlpha`/`EdgeAlpha`, 2B `scatter` için `MarkerFaceAlpha`/`MarkerEdgeAlpha`. Octave'da `bar` saydamlığı çubuğun patch'ine ayarlanır: `set(findobj(h,'type','patch'),'FaceAlpha',0.4)`.
  - Şunlar PNG olarak, nedeni yazılarak gösterilir: ışıklandırma; `scatter3` işaretçilerinde ve resimlerde saydamlık; veriyle değişen saydamlık; 8 katmandan derin saydam sahneler; `scatter`/`scatter3`'te nokta başına renk; 2B `patch`'te yüz başına renk ya da renk geçişi (`'interp'`); 3B `patch`'te üçten fazla köşeli yüzlerde renk geçişi; renk geçişli `contourf`; boyut bütçesini aşan grafikler.
- Netlist katmanı ve blok diyagramı editörü Simulink değildir: 13 blok türü ve tek örnekleme hızıyla sınırlıdır; `.slx`/`.mdl` desteği yoktur.
- MATLAB ile tam dil ya da Toolbox uyumluluğu iddia edilmez; ölçülen davranış durumları ve bilinen farklar `uyumluluk/` altında kayıtlıdır.
- Hata ayıklama duraklamasında değerlendirilen bir ifadedeki `input` yanıtını kullanıcıdan alır. Octave'ın kendi okuyucusu (`builtin('input', ...)` ya da ifadedeki `keyboard`) bu durumda yalnızca bir satır alır. Aynı ifade ikinci bir satır beklerse yalnızca Durdur kullanılabilir.
- Kod denetimi yalnızca Octave ayrıştırma hatalarını gösterir. Değişken düzenleyici verileri sayfa sayfa okur; hücre dizisi ve yapı içerikleri salt okunurdur.
- Kaydetme mekanizması hard link destekleyen bir dosya sistemi gerektirir.
- Çalıştırılan Octave kodu kullanıcının işletim sistemi yetkilerine sahiptir. Dosya panelinin sınırları kod yürütme için bir sandbox oluşturmaz.

## Geliştirme

```sh
npm ci
npm run build
```

Testler gerçek Octave ve Chromium kullanır. Gerekirse test tarayıcısını `npx playwright install chromium` ile kurun.

```sh
python3 scripts/check.py
```

**Test komutu bu proje klasöründeki açık sunucuyu kapatır. Aktif hesaplama sırasında çalıştırmayın.**

Kaynaklar: `backend/` yerel servis ve motor, `octave/` hesaplama köprüsü, `frontend/` arayüz kaynakları, `static/` hazır arayüz, `macos/` masaüstü penceresi, `scripts/` denetim ve paket araçları, `uyumluluk/` MATLAB karşılaştırma ölçümleri, `tests/` kontroller ve `workspace/examples/` örnekler.

## Lisans

Özgün uygulama kodu [MIT lisanslıdır](LICENSE). GNU Octave ve diğer bağımlılıklar kendi lisanslarına tabidir; ayrıntılar [üçüncü taraf bildirimlerinde](THIRD_PARTY_NOTICES.md) bulunur.
