# MiniTorrent

Windows için sade, hızlı ve **reklamsız** bir torrent istemcisi. Magnet linklerini ve `.torrent` dosyalarını indirir. Tek bir `.exe` olarak çalışır, kurulum gerektirmez.

Rust ile yazıldı. Torrent motoru [`librqbit`](https://github.com/ikatson/rqbit), arayüz [`egui`](https://github.com/emilk/egui) tabanlıdır.

## Özellikler

- **Magnet link desteği:** Linki yapıştırıp Enter'a basman yeterli. Tek tıkla "Panodan" düğmesiyle de eklenir.
- **`.torrent` dosyası desteği:** Dosyayı pencereye sürükleyip bırak ya da ".torrent dosyası" düğmesiyle seç.
- **Modern arayüz:** Koyu tema, torrent başına ilerleme çubuğu, boyut, yüzde, kalan süre, indirme ve yükleme hızı.
- **Sekmeler:** Tümü, İndirilenler, Tamamlanan, Duraklatılan.
- **Duraklat / devam et / kaldır:** Kaldırırken indirilen dosyaların silinip silinmeyeceğini sana sorar.
- **Hız limiti:** İndirme ve yükleme için ayrı ayrı KB/s sınırı. Değişiklik anında uygulanır.
- **Sistem tepsisi:** Pencereyi kapatınca tepsiye küçülür, indirmeler arka planda sürer.
- **Oturum hatırlama:** Program kapanıp açılınca liste geri yüklenir ve indirmeler kaldığı yerden devam eder.
- **Hız odaklı ayarlar:** DHT, UPnP ile otomatik port yönlendirme ve gelen bağlantı desteği açıktır.
- **Reklam, telemetri ve hesap yok.**

## Kullanım

### Torrent ekleme

| Yöntem | Nasıl |
|---|---|
| Magnet link | Linki üstteki kutuya yapıştır, **Ekle**'ye bas ya da Enter'a bas |
| Panodan | Linki kopyala, **Panodan** düğmesine bas |
| `.torrent` dosyası | Dosyayı pencereye sürükleyip bırak ya da **.torrent dosyası** düğmesini kullan |

İndirilen dosyalar varsayılan olarak `Downloads` klasörüne kaydedilir. Ekleme kartındaki **Değiştir** bağlantısıyla klasörü değiştirebilirsin. Değişiklik yalnızca bundan sonra eklenen torrent'lere uygulanır.

### Torrent kartı

Her kartın sağında üç düğme vardır: **duraklat/devam et**, **klasörü aç** ve **kaldır**. Karta çift tıklayınca indirme klasörü açılır. Sağ tıklayınca aynı eylemleri içeren bir menü gelir.

Kart rengi durumu gösterir:

| Renk | Anlamı |
|---|---|
| Mavi | İndiriliyor |
| Yeşil | Tamamlandı, seed ediliyor |
| Sarı | Duraklatıldı |
| Kırmızı | Hata |
| Gri | Metadata alınıyor veya kontrol ediliyor |

### Ayarlar

Sağ üstteki dişli simgesi ayarları açar.

- **Hız sınırları:** KB/s cinsinden gir. `0` sınırsız demektir. Hazır seçenekler: Sınırsız, 500 KB, 1 MB, 5 MB, 10 MB. Limit aktifken üstteki hız rozetinde mevcut hızın yanında limit de görünür.
- **Sistem tepsisi:** "Pencereyi kapatınca sistem tepsisine küçült" seçeneğini açıp kapatabilirsin.

### Sistem tepsisi

Pencereyi kapatınca program tepsiye küçülür ve indirmeler sürer. Simgeye tıklayınca pencere geri gelir. Sağ tık menüsünde **MiniTorrent'i göster** ve **Çıkış** vardır. Programı tamamen kapatmak için **Çıkış**'ı kullan.

## Derleme

### Seçenek 1: Kendi bilgisayarında

1. [rustup.rs](https://rustup.rs) adresinden Rust'ı kur. Kurulum sırasında istenirse **Visual Studio C++ Build Tools** paketini de kur ("Desktop development with C++").
2. Açık komut istemi pencerelerini kapatıp yeniden aç.
3. Proje klasöründeki `build.bat` dosyasına çift tıkla. Ya da komut isteminde şunu çalıştır:

   ```
   cargo build --release
   ```

4. Çıkan dosya: `target\release\minitorrent.exe`

İlk derleme bağımlılıkları indirdiği için 5-15 dakika sürebilir. Sonraki derlemeler çok daha hızlıdır.

### Seçenek 2: GitHub Actions ile (bilgisayara bir şey kurmadan)

1. Proje dosyalarını (`.github` klasörü dahil) bir GitHub deposuna yükle.
2. Deponun **Actions** sekmesinde "Build Windows exe" iş akışı otomatik çalışır.
3. İş akışı bitince sayfanın altındaki **Artifacts** bölümünden `minitorrent-windows` dosyasını indir. İçinden `minitorrent.exe` çıkar.

## Dosyalar ve veri konumları

Program ayarlarını ve listeni şu klasörde tutar:

```
%APPDATA%\MiniTorrent\
├── torrents.txt     Torrent listesi (kapanıp açılınca geri yüklenir)
├── settings.txt     Hız limitleri ve tepsi ayarı
└── torrents\        Eklediğin .torrent dosyalarının kopyaları
```

Programı tamamen sıfırlamak istersen bu klasörü silebilirsin. İndirilen dosyalar bu klasörde değil, seçtiğin indirme klasöründedir.

## Proje yapısı

```
minitorrent-rs/
├── Cargo.toml                 Bağımlılıklar ve derleme ayarları
├── build.rs                   Simgeyi exe dosyasına gömer
├── build.bat                  Çift tıkla derleme
├── assets/                    Simgeler (exe, pencere, tepsi, logo)
├── src/main.rs                Uygulamanın tamamı
└── .github/workflows/build.yml  Otomatik Windows derlemesi
```

## Sorun giderme

| Sorun | Çözüm |
|---|---|
| `cargo is not recognized` | Rust kurulumundan sonra komut istemini kapatıp yeniden aç ya da bilgisayarı yeniden başlat |
| `linker link.exe not found` | Visual Studio Build Tools kurulu değil. Visual Studio Installer'dan "Desktop development with C++" paketini kur |
| Windows SmartScreen uyarısı | Exe imzasız olduğu için çıkar. "Ek bilgi" ardından "Yine de çalıştır"a tıkla |
| İndirme başlamıyor, "Metadata alınıyor…" ekranında kalıyor | Magnet linkinde kaynak az olabilir. Biraz bekle, linkin geçerli olduğundan emin ol |
| İndirme yavaş | Güvenlik Duvarı'nın ağ erişimine izin verdiğinden ve ayarlardan hız limiti koymadığından emin ol |
| Tepsiye küçülmüyor | Ayarlarda tepsi seçeneğinin açık olduğunu kontrol et |

## Güvenlik duvarı

İlk çalıştırmada Windows Güvenlik Duvarı ağ erişimi için izin isteyecektir. Gelen bağlantılar ve dolayısıyla daha hızlı indirme için izin vermen gerekir.

## Yasal uyarı

MiniTorrent yalnızca bir istemci yazılımıdır. Telif hakkıyla korunan içerikleri izinsiz indirmek ya da paylaşmak birçok ülkede yasa dışıdır. Neyi indirdiğinin sorumluluğu sana aittir.

## Kullanılan teknolojiler

- [librqbit](https://github.com/ikatson/rqbit): BitTorrent motoru (DHT, magnet, UPnP)
- [egui / eframe](https://github.com/emilk/egui): arayüz
- [tray-icon](https://github.com/tauri-apps/tray-icon): sistem tepsisi
- [rfd](https://github.com/PolyMeilex/rfd): dosya ve klasör seçme penceresi
- [arboard](https://github.com/1Password/arboard): pano erişimi
