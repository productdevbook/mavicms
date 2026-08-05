# Mavi CMS

> Bu, ayrıntılı Türkçe belgelerdir. Kurulum ve genel bakış için
> [README.md](README.md) (İngilizce) daha kısadır.

Tiptap 3 tabanlı, gerçek bir Rust backend'e bağlı öz-barındırılan CMS: kurulum
sihirbazı, oturum açma, kategori/etiket, medya kütüphanesi ve yazı panosu dahil.
Vite 8 + React 19 + TanStack Router + Tailwind CSS 4 + shadcn/ui (Base UI, `base-nova`).

## Çalıştırma

```bash
bun install
bun run dev        # http://localhost:5173
bun run build      # tsc -b && vite build
bun run lint
bun run typecheck
```

## Editör özellikleri

**Metin biçimlendirme** — kalın, italik, altı/üstü çizili, satır içi kod, alt/üst simge,
metin rengi, çok renkli vurgu, yazı tipi, punto, satır aralığı, biçim temizleme.

**Bloklar** — H1–H6, paragraf, alıntı, sözdizimi vurgulu kod bloğu (lowlight, 25 dil,
blok içinden dil seçimi ve kopyalama), açılır bölüm (details), yatay ayraç, tablo
(sütun yeniden boyutlandırma, hücre birleştirme/bölme), yeniden boyutlandırılabilir görsel,
YouTube gömme.

**Listeler** — madde, numaralı ve onay kutulu görev listesi; iç içe girinti.

**Hizalama** — sol / orta / sağ / iki yana yaslama.

**Girdi yolları**

- `/` slash komut menüsü (gruplanmış, aranabilir, klavyeyle gezilir)
- `@` kişi etiketleme
- `:` emoji seçici (GitHub emoji seti)
- Markdown kısayolları ve akıllı tipografi (`--` → –, `...` → …)
- Görsel sürükle-bırak ve panodan yapıştırma

**Menüler** — seçim balonu, bağlantı balonu, görsel balonu, tablo balonu; her blok için
sürüklenebilir tutamaç ve blok menüsü (dönüştür / çoğalt / kopyala / sil).

**Araçlar** — bul ve değiştir (büyük-küçük harf duyarlılığı, tam kelime, düzenli ifade,
tümünü değiştir), içindekiler paneli, kelime/karakter/okuma süresi sayacı ve karakter
limiti göstergesi, odak modu, önizleme modu, tam ekran, açık/koyu tema.

**İçe / dışa aktarma** — HTML, Markdown, JSON ve düz metin olarak kopyalama veya indirme;
Markdown içe aktarma.

**CMS meta paneli** — durum, yayın tarihi, kalıcı bağlantı (başlıktan üretme, ilk
kayda kadar başlıkla birlikte otomatik güncellenir), gerçek kategori/etiket
(backend'de saklanır, seçiciden yeni oluşturulabilir), özet (yazıdan üretme),
kapak görseli (yükleme veya medya kütüphanesinden seçme), öne çıkarma, yorumlar,
SEO başlığı / açıklaması / canonical ve arama sonucu önizlemesi.

**Otomatik kayıt** — her değişiklik ~900ms sonra `/api/posts`'a kaydedilir
(ilk kayıtta yazı oluşturulup URL `/editor/{id}`'ye güncellenir), durum
çubuğunda kayıt göstergesi bulunur.

## Diller

Arayüz [Lingui](https://lingui.dev) ile iki dilli: İngilizce (kaynak dil) ve Türkçe
(varsayılan çalışma dili). Üst bardaki dil düğmesiyle değiştirilir, seçim
`localStorage`'da (`mavicms:locale`) saklanır. Örnek belge içeriği ve varsayılan
yazı meta verisi çeviriye dahil değildir (kullanıcı içeriği olarak kabul edilir).

Kaynak kodundaki metinler İngilizce yazılır (`t` / `<Trans>` makroları), Türkçe
çeviriler `src/locales/tr/messages.po` dosyasında tutulur.

```bash
bun run extract   # kaynaktan yeni metinleri çıkarır (src/locales/*/messages.po)
```

Yeni bir `t`/`Trans` eklendiğinde `extract` çalıştırılıp `tr/messages.po`'daki
karşılığı elle doldurulmalı. Katalog derlemesi ayrıca gerekmez —
`@lingui/vite-plugin` `.po` dosyalarını hem `dev` hem `build` sırasında anında derler.

## Rotalar

[TanStack Router](https://tanstack.com/router) ile dosya tabanlı routing (`src/routes/`):

- `/` — kurulum/oturum durumuna göre `/setup`, `/login` veya `/dashboard`'a yönlendiren boş bir uğrak noktası
- `/setup` — WordPress tarzı ilk kurulum sihirbazı (dil → **veritabanı bağlantısı** →
  site bilgisi → yönetici hesabı → kurulum) — site zaten kuruluysa otomatik olarak
  `/dashboard`'a yönlendirir
- `/login` — oturum açma; oturum gerektiren bir rotaya girişte `?redirect=` ile buraya yönlendirilir
- `/dashboard` — yazı panosu: durum sayaçları, yazı listesi, silme (bkz. Dashboard)
- `/dashboard/media` — medya kütüphanesi: yükleme, listeleme, silme
- `/dashboard/categories` — kategori yönetimi: oluşturma, silme
- `/editor/new` — yeni yazı (ilk otomatik kayıtta `/editor/{id}`'ye yönlendirir)
- `/editor/$postId` — var olan bir yazıyı düzenler

`/dashboard`, `/dashboard/media` ve `/dashboard/categories` altında ortak bir
kabuk (üst bar + gezinme) paylaşır — `dashboard.tsx` bu üçü için `<Outlet />`
içeren bir düzen (layout) rotasıdır, gerçek içerik `dashboard.index.tsx`,
`dashboard.media.tsx` ve `dashboard.categories.tsx`'te.

Route ağacı (`src/routeTree.gen.ts`) derleme sırasında otomatik üretilir, elle düzenlenmez.

## Oturum

Kurulum sihirbazı bittiğinde ve `/login`'de başarılı girişte backend
`HttpOnly` bir oturum çerezi (`mavicms_session`) kurar; `/dashboard` ve
`/editor/*` rotaları bu çerezi `GET /me` ile doğrulayıp yoksa `/login`'e
yönlendirir (bkz. `src/lib/auth-guard.ts`).

## Dashboard

- **Yazılar** (`/dashboard`) — taslak/incelemede/zamanlanmış/yayında sayaçları,
  yazı listesi (kategori + son güncelleme), düzenle/sil.
- **Medya** (`/dashboard/media`) — kare önizlemeli ızgara, çoklu yükleme, silme.
- **Kategoriler** (`/dashboard/categories`) — düz liste, oluşturma, silme.
- **Eklentiler** (`/dashboard/plugins`) — yerleşik entegrasyonlar; bkz. aşağısı.

## Eklentiler

WordPress'teki Eklentiler sayfasına benzeyen bir bölüm, ama içindekiler
uygulamaya gömülü modüllerdir — dışarıdan kod yükleyen bir eklenti sistemi
**değildir**. Şimdilik tek modül var:

**S3 uyumlu depolama** (`/dashboard/plugins/s3`) — AWS S3, Cloudflare R2, MinIO
ve DigitalOcean Spaces ile çalışır. Panelden endpoint, bölge, bucket, erişim
anahtarları, genel URL tabanı ve yol öneki girilir; kaydetmeden önce
"Bağlantıyı test et" ile küçük bir nesne yazılıp silinerek yazma izni de
doğrulanır.

- Açıkken **yalnızca yeni yüklemeler** bucket'a gider; diskteki mevcut dosyalar
  yerinde kalır ve çalışmaya devam eder.
- Görsel adresleri yazı HTML'ine gömüldüğü için **kalıcı olmak zorunda** —
  bu yüzden süreli (presigned) URL değil, `public_base_url` üzerinden kalıcı
  genel adres kullanılır (bucket public olmalı veya önünde CDN bulunmalı).
- **Kimlik bilgileri şifreli saklanır** (AES-256-GCM). Ana anahtar
  `MAVICMS_SECRET_KEY` env değişkeninden gelir; verilmezse üretilip
  `{MAVICMS_DATA_DIR}/secret_key` dosyasına `0600` izniyle yazılır. Gizli anahtar
  API'den asla geri dönmez; formda boş bırakılırsa mevcut değer korunur.
  Ayrıntılar için `backend/README.md`.

## Backend

`backend/` içinde Rust (Axum + SeaORM) ile yazılmış bir REST API var —
Postgres, MySQL veya SQLite'a bağlanır, `#[utoipa::path]` ile otomatik üretilen
OpenAPI şeması [Scalar](https://scalar.com) ile `/scalar`'da servis edilir.
İlk kurulumda veritabanı bağlantısı wizard'dan girilir (bkz. "İlk kurulum ve
veritabanı" — `backend/README.md`), ardından site başlığı + yönetici hesabı
oluşturulur (parola argon2 ile hash'lenir, oturum çerezi kurulur). Kategori,
etiket ve medya için de uçlar var; ayrıntılar için `backend/README.md`.

## Docker

Tüm proje (Postgres + Rust API + nginx arkasında statik frontend) tek komutla ayağa kalkar:

```bash
cp .env.example .env      # gerekirse DATABASE_URL / portları düzenle
docker compose up --build
```

- Frontend: `http://localhost:8081`
- API (doğrudan): `http://localhost:8080`, Scalar docs: `http://localhost:8080/scalar`
- Frontend, `/api` ve `/scalar` isteklerini nginx üzerinden backend'e proxy'ler,
  yani prod build'de frontend kodu her zaman aynı origin'e (`/api/...`) konuşur.

Kendi veritabanına bağlanmak için `.env`'de `DATABASE_URL`'i değiştir ve
istersen `docker-compose.yml`'den `postgres` servisini tamamen kaldır —
backend hangi `DATABASE_URL` verilirse ona bağlanır.

Sadece backend imajını build etmek için:

```bash
docker build -t mavicms-api ./backend
```

Sadece frontend imajını build etmek için:

```bash
docker build -t mavicms-frontend .
```

> Not: Bazı sandbox/CI ortamlarında Docker'ın varsayılan `buildx bake` build
> yolu DNS çözümlemesini engelleyebiliyor. Böyle bir hata görürsen
> `DOCKER_BUILDKIT=0 docker compose build` ile klasik builder'a düş.

## Klavye kısayolları

`Ctrl/⌘ + S` kaydet · `Ctrl/⌘ + Shift + F` bul ve değiştir · `Ctrl/⌘ + /` kısayol listesi ·
`Ctrl/⌘ + Shift + O` odak modu · `Ctrl/⌘ + Shift + Enter` tam ekran

## Yapı

```
src/
  routes/
    __root.tsx, index.tsx, setup.tsx, login.tsx
    dashboard.tsx          Panel düzeni (Outlet) — üst bar + gezinme
    dashboard.index.tsx    /dashboard — yazı listesi + sayaçlar
    dashboard.media.tsx    /dashboard/media — medya kütüphanesi
    dashboard.categories.tsx  /dashboard/categories — kategori yönetimi
    dashboard.plugins.tsx     /dashboard/plugins — eklenti listesi
    dashboard.plugins_.s3.tsx /dashboard/plugins/s3 — S3 ayarları
    editor.new.tsx, editor.$postId.tsx
  components/
    setup/
      setup-wizard.tsx   Kurulum sihirbazı (dil, veritabanı, site bilgisi, yönetici hesabı)
    dashboard/
      dashboard-shell.tsx  Panel üst barı ve gezinmesi
    editor/
      extensions/        Tiptap uzantı yapılandırması, slash komutu,
                         öneri menüleri, bul-değiştir uzantısı, dil listesi
      mavi-editor.tsx    Sayfa kabuğu, backend'e otomatik kayıt, kısayollar
      toolbar.tsx        Üst araç çubuğu
      bubble-menus.tsx   Seçim / bağlantı / görsel / tablo balonları
      block-handle.tsx   Sürükleme tutamacı ve blok menüsü
      dialogs.tsx        Görsel (medya kütüphanesi sekmeli), YouTube, bağlantı,
                         tablo, dışa aktarma, kısayollar
      find-replace.tsx   Bul ve değiştir paneli
      post-settings.tsx  Yazı meta ve SEO paneli (gerçek kategori/etiket API'sine bağlı)
      toc-panel.tsx      İçindekiler
      status-bar.tsx     Durum çubuğu
    ui/                  shadcn/ui bileşenleri
    locale-toggle.tsx    Dil değiştirici
  lib/
    api.ts               Backend fetch istemcisi (/api/...) — auth, posts,
                         kategori/etiket, medya
    auth-guard.ts        Rota koruması: oturum yoksa /login'e yönlendirir
    password.ts          Parola gücü ölçümü ve üretici
    markdown.ts          HTML ↔ Markdown dönüşümü
    editor-utils.ts      Slug, okuma süresi, dosya indirme yardımcıları
  locales/
    en/messages.po        Kaynak katalog (İngilizce)
    tr/messages.po        Türkçe çeviri
  i18n.ts                Lingui kurulumu, dil seçimi ve kalıcılığı
```

## Sonraki adımlar

- `MENTION_USERS` sabit listesi gerçek kullanıcı servisiyle değiştirilmeli
  (şu an tek yönetici hesabı var, çoklu kullanıcı/rol yok).
- WordPress'ten içerik taşıma (WXR import) — kategori, etiket, medya ve
  oturum altyapısı hazır olduğu için üstüne eklenebilir, henüz yapılmadı.
- DockerHub'a yayınlamak için: `docker build -t <kullanıcı>/mavicms-api ./backend`,
  `docker build -t <kullanıcı>/mavicms-frontend .`, ardından `docker push`.
