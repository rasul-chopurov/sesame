#!/usr/bin/env python3
"""Generates index.html, az/index.html and ru/index.html from one template.

Edit the texts in T below, then run: python3 scripts/build-website.py
"""
import html, sys, os

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "website")
SITE = "https://sesame.response.az"
GH = "https://github.com/rasul-chopurov/sesame"
REL = GH + "/releases"

ICON = {
 "arrow": '<svg class="i" viewBox="0 0 24 24"><path d="M7 17 17 7M9 7h8v8"/></svg>',
 "down": '<svg class="i" viewBox="0 0 24 24"><path d="M12 4v12m0 0-5-5m5 5 5-5M5 20h14"/></svg>',
 "eye": '<svg class="i" viewBox="0 0 24 24"><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/></svg>',
 "shield": '<svg class="i" viewBox="0 0 24 24"><path d="M12 3 4.5 6v6c0 4.4 3.2 7.9 7.5 9 4.3-1.1 7.5-4.6 7.5-9V6L12 3Z"/><path d="m9 12 2 2 4-4"/></svg>',
 "bolt": '<svg class="i" viewBox="0 0 24 24"><path d="M13 3 5 14h6l-1 7 8-11h-6l1-7Z"/></svg>',
 "spark": '<svg class="i" viewBox="0 0 24 24"><path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M6 18l2.5-2.5M15.5 8.5 18 6"/></svg>',
 "globe": '<svg class="i" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 3.8 5.6 3.8 9S14.5 18.4 12 21c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3Z"/></svg>',
 "check": '<svg class="i" viewBox="0 0 24 24"><path d="m5 12 4.5 4.5L19 7"/></svg>',
}

T = {
"en": {
 "dir": "", "lang": "en", "locale": "en_US",
 "title": "Sesame — open your servers. A native server manager for Laravel",
 "desc": "Sesame is a native, AI-friendly server manager for the Laravel community. Give it SSH and it finds every site, database, mail setup and process — then runs isolated sites, services, deploys and Cloudflare DNS for you.",
 "nav": ["Features", "Security", "CLI", "Roadmap"],
 "download": "Download for macOS", "github": "View on GitHub",
 "badge": "Alpha · v0.1.0-alpha.1 · Apache-2.0",
 "h1": "Open your <em>servers.</em>",
 "lead": "A native, AI-friendly server manager for the Laravel community. Give it SSH — it finds every site, database, mail setup and process, and runs them with you.",
 "meta": ["macOS · Windows soon", "Rust + GPUI", "No agent on your server"],
 "feat_eyebrow": "What it does", "feat_h": "Everything on the box, in one calm window.",
 "feat_p": "Sesame reads your server the way you would — over SSH, read-only first — and turns what it finds into something you can act on.",
 "cards": [
  ("Discover", "See what is really running", "One read-only probe finds sites in FrankenPHP/Caddy, Nginx and Apache, Laravel apps and their .env drivers, supervisor workers, databases, cron, certificates and the firewall — and tells you what needs attention.", ["FrankenPHP", "Octane", "Nginx", "MySQL", "PostgreSQL", "Redis", "cron", "ufw"], "overview-dark"),
  ("Isolated sites", "A new site, sealed off", "Each site gets its own Linux user, its own PHP-FPM pool or Octane, its own database user and Redis ACL. The web server can read public/ — never your .env.", ["own user", "own pool", "own DB", "Redis ACL"], "sites-light"),
  ("Services", "Databases in one click", "Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, S3 on RustFS and Reverb — bound to localhost, with generated passwords. Nothing is installed until you ask.", ["localhost only", "keep data on uninstall"], "services-dark"),
  ("Deploys", "Push to GitHub, deploy in Sesame", "Link a site to a repository and branch. Sesame creates a read-only deploy key on the server, then fetches, installs, migrates and restarts workers.", ["deploy keys", "migrations", "workers"], "site-dark"),
  ("Cloudflare", "DNS without the tab-switching", "Domains, records, proxy on or off, cache purge — and a “point to this server” button for every site that isn’t pointed yet.", ["DNS records", "proxy", "cache purge"], "cloudflare-dark"),
 ],
 "sec_eyebrow": "Safe by design", "sec_h": "Every change is shown before it happens.",
 "sec_p": "Sesame never changes a server behind your back. Plans are previewed, destructive steps ask you to type the name, and everything lands in an audit log.",
 "principles": [
  ("eye", "Read-only first", "Discovery never writes. It never reads passwords or keys from .env either."),
  ("shield", "Encrypted vault", "Secrets are sealed with XChaCha20-Poly1305 under a key derived from your master password with Argon2id."),
  ("check", "Confirm, then run", "Each plan lists its steps. Drops back up first and need you to type the name."),
  ("bolt", "Off the command line", "Passwords travel over stdin and heredocs — never in argv, never in ps."),
 ],
 "sec_shot_alt": "Confirming a destructive change",
 "cli_eyebrow": "AI-friendly", "cli_h": "Every action is a typed operation.",
 "cli_p": "The app and the sesame CLI run the same plans, so scripts and AI agents can do what you do — with the same previews and the same audit log. MCP and REST are next.",
 "cli_c": ["# save a server and look around", "# an isolated Laravel site with PostgreSQL", "# Redis, bound to localhost, password generated"],
 "pillars_eyebrow": "Native", "pillars": [("Native & fast", "Rust and GPUI. No Electron, no JVM — it opens instantly and stays light."), ("Nothing to install", "Plain SSH. No agent, no daemon, no open ports on your server."), ("Three languages", "English, Русский and Azərbaycanca, with light and dark glass themes.")],
 "road_eyebrow": "Roadmap", "road_h": "Where Sesame is going.",
 "road_done": "Shipped", "road_next": "Next",
 "done": ["Read-only discovery and insights", "Isolated new-site wizard", "One-click services", "Databases with backups", "GitHub deploys", "Cloudflare DNS", "Encrypted vault, signed updates"],
 "next": ["MCP and REST API for AI agents", "Command palette ⌘K", "Live metrics and logs", "Provision a fresh server from zero", "Windows, mobile and Sesame Cloud"],
 "final_h": "Open your servers.", "final_p": "Sesame is open source under Apache-2.0. Try the alpha on a disposable server first — and tell us what it finds.",
 "foot": "Open source under Apache-2.0", "foot_links": ["GitHub", "Releases", "Changelog", "Security"],
 "shot_alts": {"overview-dark": "Server overview with insights", "sites-light": "Sites as searchable cards", "services-dark": "One-click services", "site-dark": "A site with repository and DNS", "cloudflare-dark": "Cloudflare DNS records"},
},
"az": {
 "dir": "az/", "lang": "az", "locale": "az_AZ",
 "title": "Sesame — serverlərinizi açın. Laravel üçün nativ server meneceri",
 "desc": "Sesame Laravel icması üçün nativ, süni intellektə uyğun server meneceridir. SSH girişi verin — o, bütün saytları, bazaları, poçtu və prosesləri tapır, sonra izolə edilmiş saytları, xidmətləri, deployları və Cloudflare DNS-i idarə edir.",
 "nav": ["İmkanlar", "Təhlükəsizlik", "CLI", "Yol xəritəsi"],
 "download": "macOS üçün yüklə", "github": "GitHub-da bax",
 "badge": "Alfa · v0.1.0-alpha.1 · Apache-2.0",
 "h1": "Serverlərinizi <em>açın.</em>",
 "lead": "Laravel icması üçün nativ, süni intellektə uyğun server meneceri. SSH verin — bütün saytları, bazaları, poçtu və prosesləri tapır və onları sizinlə birlikdə idarə edir.",
 "meta": ["macOS · tezliklə Windows", "Rust + GPUI", "Serverdə agent yoxdur"],
 "feat_eyebrow": "Nə edir", "feat_h": "Serverdəki hər şey bir sakit pəncərədə.",
 "feat_p": "Sesame serverinizi sizin kimi oxuyur — SSH ilə, əvvəlcə yalnız oxuma rejimində — və tapdıqlarını idarə edilə bilən bir mənzərəyə çevirir.",
 "cards": [
  ("Kəşf", "Həqiqətən nə işlədiyini görün", "Bir oxuma yoxlaması FrankenPHP/Caddy, Nginx və Apache-dəki saytları, Laravel tətbiqlərini və onların .env drayverlərini, supervisor proseslərini, bazaları, cron-u, sertifikatları və firewall-u tapır — və nəyin diqqət tələb etdiyini göstərir.", ["FrankenPHP", "Octane", "Nginx", "MySQL", "PostgreSQL", "Redis", "cron", "ufw"], "overview-dark"),
  ("İzolə edilmiş saytlar", "Yeni sayt, tam ayrılmış", "Hər sayt öz Linux istifadəçisini, öz PHP-FPM pulunu və ya Octane-ı, öz baza istifadəçisini və Redis ACL-ni alır. Veb server yalnız public/ qovluğunu oxuyur — .env faylınızı heç vaxt.", ["öz istifadəçi", "öz pul", "öz baza", "Redis ACL"], "sites-light"),
  ("Xidmətlər", "Bazalar bir kliklə", "Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, RustFS üzərində S3 və Reverb — yalnız localhost-da, yaradılmış parollarla. Siz istəməyincə heç nə quraşdırılmır.", ["yalnız localhost", "silərkən məlumat qalır"], "services-dark"),
  ("Deploylar", "GitHub-a push, Sesame-də deploy", "Saytı repozitoriya və budaqla əlaqələndirin. Sesame serverdə yalnız oxuma üçün deploy açarı yaradır, sonra kodu çəkir, asılılıqları quraşdırır, miqrasiyaları işə salır və prosesləri yenidən başladır.", ["deploy açarları", "miqrasiyalar", "workerlər"], "site-dark"),
  ("Cloudflare", "Tablar arasında keçid olmadan DNS", "Domenlər, qeydlər, proksi aç/bağla, keşin təmizlənməsi — və hələ serverə yönəlməmiş hər sayt üçün «bu serverə yönəlt» düyməsi.", ["DNS qeydləri", "proksi", "keş"], "cloudflare-dark"),
 ],
 "sec_eyebrow": "Təhlükəsiz dizayn", "sec_h": "Hər dəyişiklik baş verməzdən əvvəl göstərilir.",
 "sec_p": "Sesame serveri xəbəriniz olmadan dəyişmir. Planlar əvvəlcədən göstərilir, geri qaytarılmaz addımlar adı yazmağınızı tələb edir və hər şey audit jurnalına düşür.",
 "principles": [
  ("eye", "Əvvəlcə yalnız oxuma", "Kəşf heç nə yazmır. .env-dən parol və açarları da oxumur."),
  ("shield", "Şifrəli anbar", "Sirlər master paroldan Argon2id ilə alınan açarla XChaCha20-Poly1305 vasitəsilə şifrələnir."),
  ("check", "Təsdiqlə, sonra icra et", "Hər plan addımlarını göstərir. Silmədən əvvəl ehtiyat nüsxə çıxarılır və adı yazmaq lazımdır."),
  ("bolt", "Komanda sətrindən kənarda", "Parollar stdin və heredoc ilə ötürülür — heç vaxt argv-də, heç vaxt ps-də görünmür."),
 ],
 "sec_shot_alt": "Geri qaytarılmaz dəyişikliyin təsdiqi",
 "cli_eyebrow": "Süni intellektə uyğun", "cli_h": "Hər əməliyyat tipli əməliyyatdır.",
 "cli_p": "Tətbiq və sesame CLI eyni planları icra edir — skriptlər və süni intellekt agentləri də sizin etdiyinizi edə bilər, eyni ön baxış və eyni audit jurnalı ilə. Növbəti addım MCP və REST-dir.",
 "cli_c": ["# serveri yadda saxla və yoxla", "# PostgreSQL ilə izolə edilmiş Laravel saytı", "# Redis, yalnız localhost-da, parol yaradılır"],
 "pillars_eyebrow": "Nativ", "pillars": [("Nativ və sürətli", "Rust və GPUI. Electron yox, JVM yox — dərhal açılır və yüngül qalır."), ("Heç nə quraşdırmaq lazım deyil", "Adi SSH. Serverdə agent, xidmət və ya açıq port yoxdur."), ("Üç dil", "English, Русский və Azərbaycanca, açıq və tünd şüşə mövzuları ilə.")],
 "road_eyebrow": "Yol xəritəsi", "road_h": "Sesame hara gedir.",
 "road_done": "Hazırdır", "road_next": "Növbəti",
 "done": ["Yalnız oxuma kəşfi və məsləhətlər", "İzolə edilmiş yeni sayt ustası", "Bir kliklə xidmətlər", "Ehtiyat nüsxəli bazalar", "GitHub deployları", "Cloudflare DNS", "Şifrəli anbar, imzalı yeniləmələr"],
 "next": ["Süni intellekt agentləri üçün MCP və REST API", "Komanda paneli ⌘K", "Canlı metrikalar və loglar", "Sıfırdan yeni server qurulması", "Windows, mobil və Sesame Cloud"],
 "final_h": "Serverlərinizi açın.", "final_p": "Sesame Apache-2.0 lisenziyası ilə açıq mənbəlidir. Alfa versiyanı əvvəlcə test serverində sınayın — və nə tapdığını bizə deyin.",
 "foot": "Açıq mənbə, Apache-2.0", "foot_links": ["GitHub", "Buraxılışlar", "Dəyişikliklər", "Təhlükəsizlik"],
 "shot_alts": {"overview-dark": "Məsləhətlərlə server icmalı", "sites-light": "Axtarışlı sayt kartları", "services-dark": "Bir kliklə xidmətlər", "site-dark": "Repozitoriya və DNS ilə sayt", "cloudflare-dark": "Cloudflare DNS qeydləri"},
},
"ru": {
 "dir": "ru/", "lang": "ru", "locale": "ru_RU",
 "title": "Sesame — откройте свои серверы. Нативный менеджер серверов для Laravel",
 "desc": "Sesame — нативный менеджер серверов для Laravel-сообщества, удобный для ИИ-агентов. Дайте SSH — он найдёт все сайты, базы, почту и процессы, а затем поможет вести изолированные сайты, сервисы, деплои и DNS в Cloudflare.",
 "nav": ["Возможности", "Безопасность", "CLI", "Планы"],
 "download": "Скачать для macOS", "github": "Открыть на GitHub",
 "badge": "Альфа · v0.1.0-alpha.1 · Apache-2.0",
 "h1": "Откройте свои <em>серверы.</em>",
 "lead": "Нативный менеджер серверов для Laravel-сообщества, удобный для ИИ-агентов. Дайте SSH — он найдёт все сайты, базы, почту и процессы и поможет ими управлять.",
 "meta": ["macOS · скоро Windows", "Rust + GPUI", "Без агента на сервере"],
 "feat_eyebrow": "Что умеет", "feat_h": "Всё, что есть на сервере, в одном спокойном окне.",
 "feat_p": "Sesame смотрит на сервер так же, как вы, — по SSH и сначала только на чтение — и превращает найденное в понятную картину, с которой можно работать.",
 "cards": [
  ("Обнаружение", "Видно, что на самом деле работает", "Одна проверка только на чтение находит сайты в FrankenPHP/Caddy, Nginx и Apache, Laravel-приложения и драйверы из их .env, процессы supervisor, базы, cron, сертификаты и файрвол — и подсказывает, что требует внимания.", ["FrankenPHP", "Octane", "Nginx", "MySQL", "PostgreSQL", "Redis", "cron", "ufw"], "overview-dark"),
  ("Изолированные сайты", "Новый сайт — в своей комнате", "У каждого сайта свой Linux-пользователь, свой пул PHP-FPM или Octane, свой пользователь базы и Redis ACL. Веб-сервер читает public/ — но никогда ваш .env.", ["свой пользователь", "свой пул", "своя база", "Redis ACL"], "sites-light"),
  ("Сервисы", "Базы в один клик", "Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, S3 на RustFS и Reverb — только на localhost, со сгенерированными паролями. Ничего не ставится, пока вы не попросите.", ["только localhost", "данные сохраняются при удалении"], "services-dark"),
  ("Деплои", "Push в GitHub — деплой в Sesame", "Привяжите сайт к репозиторию и ветке. Sesame создаст на сервере deploy-ключ только на чтение, затем заберёт код, поставит зависимости, выполнит миграции и перезапустит воркеры.", ["deploy-ключи", "миграции", "воркеры"], "site-dark"),
  ("Cloudflare", "DNS без лишних вкладок", "Домены, записи, прокси вкл/выкл, очистка кэша — и кнопка «направить на этот сервер» для каждого сайта, который ещё туда не смотрит.", ["DNS-записи", "прокси", "кэш"], "cloudflare-dark"),
 ],
 "sec_eyebrow": "Безопасность по умолчанию", "sec_h": "Каждое изменение видно до того, как оно произойдёт.",
 "sec_p": "Sesame никогда не меняет сервер за вашей спиной. План показывается заранее, необратимые шаги просят ввести имя, и всё попадает в журнал действий.",
 "principles": [
  ("eye", "Сначала только чтение", "Обнаружение ничего не записывает и не читает пароли и ключи из .env."),
  ("shield", "Зашифрованное хранилище", "Секреты шифруются XChaCha20-Poly1305 ключом, выведенным из мастер-пароля через Argon2id."),
  ("check", "Подтвердить, потом выполнить", "Каждый план перечисляет шаги. Перед удалением — бэкап и ввод имени."),
  ("bolt", "Не в командной строке", "Пароли идут через stdin и heredoc — никогда в argv и никогда в ps."),
 ],
 "sec_shot_alt": "Подтверждение необратимого изменения",
 "cli_eyebrow": "Для ИИ-агентов", "cli_h": "Каждое действие — типизированная операция.",
 "cli_p": "Приложение и CLI sesame выполняют одни и те же планы, поэтому скрипты и ИИ-агенты могут делать то же, что и вы, — с тем же предпросмотром и тем же журналом. Следом — MCP и REST.",
 "cli_c": ["# сохранить сервер и осмотреться", "# изолированный Laravel-сайт с PostgreSQL", "# Redis только на localhost, пароль сгенерирован"],
 "pillars_eyebrow": "Нативно", "pillars": [("Нативный и быстрый", "Rust и GPUI. Без Electron и JVM — открывается мгновенно и остаётся лёгким."), ("Ничего не ставить", "Обычный SSH. Ни агента, ни демона, ни открытых портов на сервере."), ("Три языка", "English, Русский и Azərbaycanca, светлая и тёмная стеклянные темы.")],
 "road_eyebrow": "Планы", "road_h": "Куда идёт Sesame.",
 "road_done": "Уже есть", "road_next": "Дальше",
 "done": ["Обнаружение только на чтение и подсказки", "Мастер изолированного сайта", "Сервисы в один клик", "Базы с бэкапами", "Деплои из GitHub", "DNS в Cloudflare", "Зашифрованное хранилище, подписанные обновления"],
 "next": ["MCP и REST API для ИИ-агентов", "Командная палитра ⌘K", "Живые метрики и логи", "Подготовка нового сервера с нуля", "Windows, мобильная версия и Sesame Cloud"],
 "final_h": "Откройте свои серверы.", "final_p": "Sesame — открытый код под лицензией Apache-2.0. Попробуйте альфу сначала на тестовом сервере и расскажите, что он нашёл.",
 "foot": "Открытый код, Apache-2.0", "foot_links": ["GitHub", "Релизы", "Изменения", "Безопасность"],
 "shot_alts": {"overview-dark": "Обзор сервера с подсказками", "sites-light": "Сайты карточками с поиском", "services-dark": "Сервисы в один клик", "site-dark": "Сайт с репозиторием и DNS", "cloudflare-dark": "DNS-записи в Cloudflare"},
},
}

e = html.escape

def page(code):
    t = T[code]
    up = "../" if t["dir"] else ""
    shot = lambda name: f"{up}assets/screens/{name}-{code}.webp"
    alts = "\n".join(f'  <link rel="alternate" hreflang="{c}" href="{SITE}/{T[c]["dir"]}">' for c in T)
    parts = []
    for c in ["en", "az", "ru"]:
        href = (up + T[c]["dir"]) or "./"
        cur = ' aria-current="page"' if c == code else ""
        parts.append(f'<a href="{href}" lang="{c}" hreflang="{c}"{cur}>{c.upper()}</a>')
    langs = "".join(parts)
    cards_html = []
    layout = ["wide", "narrow", "narrow", "wide", "full"]
    for i, (tag, h, p, chips, img) in enumerate(t["cards"]):
        chips_html = "".join(f"<li>{e(c)}</li>" for c in chips)
        cards_html.append(f'''
      <article class="card {layout[i]} bezel reveal{' d1' if i % 2 else ''}">
        <div class="core">
          <div class="text">
            <span class="tag">{e(tag)}</span>
            <h3>{e(h)}</h3>
            <p>{e(p)}</p>
            <ul>{chips_html}</ul>
          </div>
          <figure class="shot"><img src="{shot(img)}" width="1600" height="1018" loading="lazy" decoding="async" alt="{e(t['shot_alts'][img])}"></figure>
        </div>
      </article>''')
    principles = "".join(f'''
      <div class="bezel reveal d{i % 3}"><div class="core">
        <span class="glyph">{ICON[ic]}</span>
        <h3>{e(h)}</h3>
        <p>{e(p)}</p>
      </div></div>''' for i, (ic, h, p) in enumerate(t["principles"]))
    pillars = "".join(f'''
      <div class="bezel reveal d{i}"><div class="core">
        <span class="glyph">{ICON[["bolt","globe","spark"][i]]}</span>
        <h3>{e(h)}</h3>
        <p>{e(p)}</p>
      </div></div>''' for i, (h, p) in enumerate(t["pillars"]))
    done = "".join(f"<li>{e(x)}</li>" for x in t["done"])
    nxt = "".join(f"<li>{e(x)}</li>" for x in t["next"])
    c = t["cli_c"]
    foot_urls = [GH, REL, GH + "/blob/main/CHANGELOG.md", GH + "/blob/main/SECURITY.md"]
    foot_links = "".join(f'<a href="{u}">{e(n)}</a>' for n, u in zip(t["foot_links"], foot_urls))
    ids = ["features", "security", "cli", "roadmap"]
    nav_links = "".join(f'<a href="#{i}">{e(n)}</a>' for i, n in zip(ids, t["nav"]))
    return f'''<!doctype html>
<html lang="{t['lang']}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>{e(t['title'])}</title>
  <meta name="description" content="{e(t['desc'])}">
  <link rel="canonical" href="{SITE}/{t['dir']}">
{alts}
  <link rel="alternate" hreflang="x-default" href="{SITE}/">
  <meta name="theme-color" content="#07060b" media="(prefers-color-scheme: dark)">
  <meta name="theme-color" content="#fbf6ee" media="(prefers-color-scheme: light)">
  <link rel="icon" href="{up}assets/logo.svg" type="image/svg+xml">
  <link rel="apple-touch-icon" href="{up}assets/logo.svg">
  <meta property="og:type" content="website">
  <meta property="og:site_name" content="Sesame">
  <meta property="og:locale" content="{t['locale']}">
  <meta property="og:title" content="{e(t['title'])}">
  <meta property="og:description" content="{e(t['desc'])}">
  <meta property="og:url" content="{SITE}/{t['dir']}">
  <meta property="og:image" content="{SITE}/assets/og.jpg">
  <meta name="twitter:card" content="summary_large_image">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600&family=Geist+Mono:wght@400;500;600&display=swap" rel="stylesheet">
  <link rel="stylesheet" href="{up}assets/site.css">
  <link rel="preload" as="image" href="{shot('overview-dark')}" type="image/webp">
</head>
<body>
  <div class="backdrop" aria-hidden="true"><span></span></div>
  <div class="grain" aria-hidden="true"></div>

  <header class="nav" aria-label="Sesame">
    <a class="brand" href="{up}{t['dir']}"><img src="{up}assets/logo.svg" alt="" width="26" height="26"><span>Sesame</span></a>
    <nav class="nav-links" aria-label="Sections">{nav_links}</nav>
    <div class="lang" role="navigation" aria-label="Language">{langs}</div>
    <a class="btn btn-primary btn-sm" href="{REL}">{e(t['download'])}<span class="ico">{ICON['down']}</span></a>
  </header>

  <main>
    <section class="hero" aria-labelledby="hero-title">
      <div class="wrap hero-grid">
        <div class="reveal">
          <span class="eyebrow"><span class="dot"></span>{e(t['badge'])}</span>
          <h1 id="hero-title">{t['h1']}</h1>
          <p class="lead">{e(t['lead'])}</p>
          <div class="cta">
            <a class="btn btn-primary" href="{REL}">{e(t['download'])}<span class="ico">{ICON['down']}</span></a>
            <a class="btn btn-ghost" href="{GH}">{e(t['github'])}<span class="ico">{ICON['arrow']}</span></a>
          </div>
          <div class="meta">{"".join(f"<span>{e(m)}</span>" for m in t['meta'])}</div>
        </div>
        <div class="hero-shot reveal d2">
          <div class="bezel"><div class="core"><img src="{shot('overview-dark')}" width="1600" height="1018" fetchpriority="high" alt="{e(t['shot_alts']['overview-dark'])}"></div></div>
          <div class="float bezel"><div class="core"><img src="{shot('job-done-light')}" width="1600" height="1018" loading="lazy" alt=""></div></div>
        </div>
      </div>
    </section>

    <section id="features" aria-labelledby="features-title">
      <div class="wrap">
        <div class="section-head reveal">
          <span class="eyebrow"><span class="dot"></span>{e(t['feat_eyebrow'])}</span>
          <h2 id="features-title">{e(t['feat_h'])}</h2>
          <p>{e(t['feat_p'])}</p>
        </div>
        <div class="bento">{"".join(cards_html)}
        </div>
      </div>
    </section>

    <section id="security" aria-labelledby="security-title">
      <div class="wrap">
        <div class="section-head reveal">
          <span class="eyebrow"><span class="dot"></span>{e(t['sec_eyebrow'])}</span>
          <h2 id="security-title">{e(t['sec_h'])}</h2>
          <p>{e(t['sec_p'])}</p>
        </div>
        <div class="principles">{principles}
        </div>
        <div class="bezel reveal" style="margin-top:18px"><div class="core"><img src="{shot('confirm-drop-dark')}" width="1600" height="1018" loading="lazy" alt="{e(t['sec_shot_alt'])}"></div></div>
      </div>
    </section>

    <section id="cli" aria-labelledby="cli-title">
      <div class="wrap cli-grid">
        <div class="section-head reveal" style="margin:0">
          <span class="eyebrow"><span class="dot"></span>{e(t['cli_eyebrow'])}</span>
          <h2 id="cli-title">{e(t['cli_h'])}</h2>
          <p>{e(t['cli_p'])}</p>
        </div>
        <div class="bezel reveal d1"><div class="core">
          <div class="term-bar" aria-hidden="true"><i></i><i></i><i></i></div>
<pre class="term"><code><span class="c">{e(c[0])}</span>
<span class="p">$</span> <b>sesame add</b> prod root@203.0.113.10
<span class="p">$</span> <b>sesame discover</b> prod

<span class="c">{e(c[1])}</span>
<span class="p">$</span> <b>sesame do</b> prod new-site shop.example.com --db postgres --yes

<span class="c">{e(c[2])}</span>
<span class="p">$</span> <b>sesame do</b> prod install redis --yes</code></pre>
        </div></div>
      </div>
      <div class="wrap principles pillars-row">{pillars}
      </div>
    </section>

    <section id="roadmap" aria-labelledby="roadmap-title">
      <div class="wrap">
        <div class="section-head reveal">
          <span class="eyebrow"><span class="dot"></span>{e(t['road_eyebrow'])}</span>
          <h2 id="roadmap-title">{e(t['road_h'])}</h2>
        </div>
        <div class="road">
          <div class="bezel reveal"><div class="core done"><h3>{ICON['check']} {e(t['road_done'])}</h3><ul>{done}</ul></div></div>
          <div class="bezel reveal d1"><div class="core"><h3>{ICON['spark']} {e(t['road_next'])}</h3><ul>{nxt}</ul></div></div>
        </div>
      </div>
    </section>

    <section class="final" aria-labelledby="final-title">
      <div class="wrap reveal">
        <img src="{up}assets/logo.svg" alt="" width="72" height="72" style="margin:0 auto 26px;border-radius:18px">
        <h2 id="final-title">{e(t['final_h'])}</h2>
        <p>{e(t['final_p'])}</p>
        <div class="cta">
          <a class="btn btn-primary" href="{REL}">{e(t['download'])}<span class="ico">{ICON['down']}</span></a>
          <a class="btn btn-ghost" href="{GH}">{e(t['github'])}<span class="ico">{ICON['arrow']}</span></a>
        </div>
      </div>
    </section>
  </main>

  <footer>
    <div class="wrap">
      <span>© 2026 Sesame · {e(t['foot'])}</span>
      <nav aria-label="Footer">{foot_links}</nav>
    </div>
  </footer>
  <script src="{up}assets/site.js" defer></script>
</body>
</html>
'''

for code in T:
    d = os.path.join(OUT, T[code]["dir"])
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "index.html"), "w", encoding="utf-8") as f:
        f.write(page(code))
print("ok")
