# IndyMAT marka kiti

## Dosyalar

```
logo/svg/        amblem, yazı logosu, dikey ve yatay logo; her biri -black ve -white
logo/png/        aynı dosyaların şeffaf zeminli PNG'leri (amblem 16–1024px, logolar 512–2048px)
app-icon/svg/    uygulama ikonu: light, dark, grey + köşesiz -fullbleed sürümleri
app-icon/png/    1024 / 512 / 256 px
app-icon/web/    favicon.ico, favicon.svg, apple-touch-icon.png, icon-192, icon-512, icon-maskable-512
fonts/web/       Montserrat 400/500/600 woff2 (latin + latin-ext, Türkçe karakterler dahil) ve montserrat.css
fonts/OFL.txt    Montserrat lisansı
tokens/          brand.css (CSS değişkenleri ve yazı stilleri), brand.json (renk, yazı, logo ölçüleri)
```

SVG'lerdeki yazılar eğriye çevrilmiştir; yazı tipi kurulu olmasa da doğru görünür.

## Siteye ekleme

```html
<link rel="icon" href="/favicon.ico" sizes="48x48">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="stylesheet" href="/tokens/brand.css">
```

## Yazı tipi

Montserrat, SIL Open Font License 1.1. Masaüstüne kurmak için: https://fonts.google.com/specimen/Montserrat

| Kullanım | Ağırlık | Harf aralığı | Not |
|---|---|---|---|
| Marka adı (wordmark) | SemiBold 600 | 0.14em | `IndyMAT` |
| Slogan | Regular 400 | 0.819em | büyük harf, yazı logosu genişliğine yaslı |
| Etiket | Medium 500 | 0.3em | büyük harf, 11px |
| Başlık | SemiBold 600 | 0 / -0.01em | 24px ve 40px |
| Gövde | Regular 400 | 0 | 16px / 1.6 |
| Kod (öneri) | JetBrains Mono 400 | 0 | 14px / 1.6 |

## Renk

| Ad | HEX | RGB | CMYK | Kullanım |
|---|---|---|---|---|
| ink | #141516 | 20 21 22 | 9 5 0 91 | amblem ve metin, açık zeminde |
| graphite | #101112 | 16 17 18 | 11 6 0 93 | koyu zemin |
| paper | #FAFAFA | 250 250 250 | 0 0 0 2 | açık zemin |
| white | #FFFFFF | 255 255 255 | 0 0 0 0 | amblem ve metin, koyu zeminde |
| mist | #E4E5E6 | 228 229 230 | 1 0 0 10 | gri ikon zemini, ince çizgi |
| slate | #686A6F | 104 106 111 | 6 5 0 56 | ikincil metin, açık zeminde |
| slate-light | #8C9094 | 140 144 148 | 5 3 0 42 | ikincil metin, koyu zeminde |

CMYK değerleri RGB'den hesaplanmış yaklaşık değerlerdir; baskıdan önce matbaanın profiliyle doğrula.

## Logo kuralları

- Amblem 30° izometrik ızgarada 12u × 26v ölçüsündedir (v = u · tan 30°); iki parça merkeze göre 180° simetrik, aralarındaki kanal her yerde 1u.
- Koruma alanı: dört yanda amblem genişliğinin dörtte biri (3u).
- En küçük boyut: amblem 16px (baskıda 6mm), yatay logo 96px, sloganlı dikey logo 240px (baskıda 45mm). Daha küçükte sloganı at.
- `-black` dosyaları paper, white ve mist üzerinde; `-white` dosyaları graphite ve ink üzerinde.
- Oranları bozma, döndürme, eğme. Gölge, degrade, kontur ekleme. Parçaları farklı renge boyama, aradaki kanalı kapatma.
- Ad her zaman `IndyMAT`: `Indymat`, `INDYMAT`, `Indy MAT` değil.
