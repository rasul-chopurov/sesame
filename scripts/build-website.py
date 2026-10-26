#!/usr/bin/env python3
"""Generates index.html, az/index.html and ru/index.html from one template.

Edit the texts in T below, then run: python3 scripts/build-website.py
"""
import html, sys, os

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "website")
SITE = "https://sesame.response.az"
GH = "https://github.com/rasul-chopurov/sesame"
REL = GH + "/releases"
VERSION = "v0.1.0-alpha.1"

e = html.escape

import hashlib
def _ver(name):
    path = os.path.join(OUT, "assets", name)
    try:
        return hashlib.sha256(open(path, "rb").read()).hexdigest()[:10]
    except OSError:
        return "0"
CSS_V, JS_V = _ver("site.css"), _ver("site.js")

T = {
"en": {
 "dir": "", "lang": "en", "locale": "en_US",
 "title": "Sesame — open your servers. A native server manager for Laravel",
 "desc": "Sesame is a native, open-source server manager for the Laravel community. Give it SSH: it finds every site, database and process, runs isolated sites, services, deploys and Cloudflare DNS, and lets AI agents help — with your approval.",
 "nav": ["How it works", "Features", "Agents", "Security", "Pricing", "FAQ"],
 "download": "Download Sesame", "github": "GitHub",
 "eyebrow": "Open source · Apache-2.0 · " + VERSION,
 "h1": "Open your <em>servers.</em>",
 "lead": "A native server manager for Laravel. Give it SSH — it finds every site, database and process on the box, and runs them with you.",
 "micro": "Free and open source. macOS today, Windows next. Nothing to install on your server.",
 "hero_alt": "Sesame showing a server overview with insights",
 "how_eyebrow": "How it works", "how_h": "From an SSH login to a calm picture in a minute.",
 "how": [
  ("Minute one", "Connect", "Your ssh-agent, a key or a password. Host keys are checked against known_hosts. No agent, no daemon, no open port."),
  ("Minute two", "Discover", "One read-only probe finds sites in Caddy, Nginx and Apache, Laravel apps, workers, databases, cron, certificates and the firewall."),
  ("When you're ready", "Run", "New isolated sites, one-click services, databases, deploys from GitHub and Cloudflare DNS — each change previewed first."),
 ],
 "probe": [("ok", "frankenphp", "3 sites · Octane"), ("ok", "postgresql 18", "4 databases · 212 MB"), ("ok", "redis", "ACL per site"), ("ok", "supervisor", "queue ×2 · reverb"), ("warn", "ufw", "inactive"), ("warn", "certificate", "shop.example.com · 9 days")],
 "f": [
  ("Isolated sites", "Every site in its own room.", "Each new site gets its own Linux user, its own PHP-FPM pool or Octane process, its own database user and a Redis ACL limited to its keys. The web server reads public/ — never your .env.", ["own user", "own pool", "own DB", "Redis ACL", "subdomains"], "sites-light", "Sites as searchable cards"),
  ("Services", "Databases in one click. Nothing until you ask.", "Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, S3 on RustFS and Reverb. Bound to localhost, passwords generated and sealed in the vault. phpMyAdmin and pgAdmin open through an SSH tunnel, never in public.", ["localhost only", "keep data on uninstall", "admin panels via tunnel"], "services-dark", "One-click services"),
  ("Deploys & DNS", "Push to GitHub. Point the domain. Done.", "Link a site to a repository and branch: Sesame adds a read-only deploy key, fetches, installs, migrates and restarts workers. Cloudflare zones and records live next to the site, with a “point to this server” button.", ["deploy keys", "migrations", "DNS records", "cache purge"], "cloudflare-dark", "Cloudflare DNS records"),
  ("Files & terminal", "When you need your hands on it.", "A file manager jailed to one site over SFTP, with a small editor. A real terminal, xterm-256color, in the same window.", ["SFTP jail", "editor", "PTY terminal"], "site-dark", "A site with repository and DNS"),
 ],
 "agents_eyebrow": "AI agents", "agents_h": "Let Claude run your servers. On your terms.",
 "agents_p": "Sesame speaks MCP and REST. Agents read everything at once; every change pops up in the app, and nothing runs until you press Allow. Every step lands in the audit log with the agent's name.",
 "surfaces": ["MCP", "REST", "CLI"],
 "approve_who": "Claude Code wants to:", "approve_what": "install Redis on prod", "approve_d": ["listens on 127.0.0.1:6379", "password generated, stored in the vault"], "approve_deny": "Deny", "approve_allow": "Allow",
 "nums": [("30", "agent tools"), ("8", "one-click services"), ("3", "languages"), ("0", "agents on your server")],
 "trust_eyebrow": "Local by design", "trust_h": "Your servers, your Mac, your keys.",
 "trust": [
  ("Read-only first", "Discovery never writes, and never reads passwords or keys from .env."),
  ("Encrypted vault", "XChaCha20-Poly1305 under a key from your master password via Argon2id."),
  ("Confirm, then run", "Plans list their steps. Drops back up first and ask you to type the name."),
  ("Off the command line", "Secrets travel over stdin — never in argv, never in ps."),
  ("Signed updates", "Releases are verified with ed25519 before they replace the app."),
 ],
 "price_eyebrow": "Pricing", "price_h": "Free where it runs on your machine.",
 "plans": [
  ("Self-hosted", "$0", "forever", ["Unlimited servers and sites", "MCP and REST for agents", "Cloudflare and GitHub", "Apache-2.0, fork it"], "Download Sesame", True),
  ("Sesame Cloud", "Later", "for teams", ["Servers shared with your team", "Web and mobile dashboards", "Roles, approvals and audit in one place", "Alerts and scheduled backups"], "Follow on GitHub", False),
 ],
 "price_micro": "The desktop app stays free and open source. Cloud will pay for the hosting, not for the features you already have.",
 "faq_h": "Questions",
 "faq": [
  ("How is this different from Forge or Ploi?", "They are web dashboards that keep access to your servers. Sesame is an app on your Mac that talks plain SSH, so your keys stay with you and it can adopt a server you set up by hand."),
  ("And from Coolify?", "Coolify runs on the server and builds containers. Sesame installs nothing on the server and manages the classic Laravel stack: Caddy or Nginx, PHP-FPM or Octane, supervisor, system databases."),
  ("What does it change on my server?", "Nothing during discovery. Everything else is a plan you see first — with destructive steps asking you to type the name, and a backup before any drop."),
  ("Where are my passwords?", "In an encrypted vault on your Mac, unlocked with your master password or a key file. Never in the plain database, never in logs."),
  ("Which servers are supported?", "Ubuntu 22.04/24.04 and Debian 12, amd64 and arm64. Discovery works on most Linux servers."),
 ],
 "final_h": "Open your servers.", "final_p": "Try the alpha on a test server first — and tell us what it found.",
 "foot_about": "Sesame is a native, AI-friendly server manager for the Laravel community, written in Rust with GPUI. It discovers what runs on a server over SSH and manages isolated sites, services, databases, deploys and Cloudflare DNS.",
 "foot_cols": [("Product", [("Releases", REL), ("Changelog", GH + "/blob/main/CHANGELOG.md"), ("Roadmap", GH + "#roadmap")]), ("Project", [("GitHub", GH), ("Contributing", GH + "/blob/main/CONTRIBUTING.md"), ("Security", GH + "/blob/main/SECURITY.md")])],
 "foot_copy": "Open source, Apache-2.0",
},
"ru": {
 "dir": "ru/", "lang": "ru", "locale": "ru_RU",
 "title": "Sesame — откройте свои серверы. Нативный менеджер серверов для Laravel",
 "desc": "Sesame — нативный менеджер серверов с открытым кодом для Laravel-сообщества. Дайте SSH: он найдёт все сайты, базы и процессы, поможет вести изолированные сайты, сервисы, деплои и DNS в Cloudflare, а ИИ-агенты работают только с вашего разрешения.",
 "nav": ["Как работает", "Возможности", "Агенты", "Безопасность", "Цены", "Вопросы"],
 "download": "Скачать Sesame", "github": "GitHub",
 "eyebrow": "Открытый код · Apache-2.0 · " + VERSION,
 "h1": "Откройте свои <em>серверы.</em>",
 "lead": "Нативный менеджер серверов для Laravel. Дайте SSH — он найдёт все сайты, базы и процессы на сервере и поможет ими управлять.",
 "micro": "Бесплатно и с открытым кодом. Сейчас macOS, следом Windows. На сервер ничего ставить не нужно.",
 "hero_alt": "Sesame: обзор сервера с подсказками",
 "how_eyebrow": "Как работает", "how_h": "От входа по SSH до понятной картины за минуту.",
 "how": [
  ("Первая минута", "Подключение", "ssh-agent, ключ или пароль. Ключ хоста сверяется с known_hosts. Ни агента, ни демона, ни открытого порта."),
  ("Вторая минута", "Обнаружение", "Одна проверка только на чтение находит сайты в Caddy, Nginx и Apache, Laravel-приложения, воркеры, базы, cron, сертификаты и файрвол."),
  ("Когда будете готовы", "Управление", "Новые изолированные сайты, сервисы в один клик, базы, деплои из GitHub и DNS в Cloudflare — каждое изменение сначала показывается."),
 ],
 "probe": [("ok", "frankenphp", "3 сайта · Octane"), ("ok", "postgresql 18", "4 базы · 212 МБ"), ("ok", "redis", "ACL на сайт"), ("ok", "supervisor", "очередь ×2 · reverb"), ("warn", "ufw", "выключен"), ("warn", "сертификат", "shop.example.com · 9 дней")],
 "f": [
  ("Изолированные сайты", "Каждый сайт — в своей комнате.", "У каждого нового сайта свой Linux-пользователь, свой пул PHP-FPM или процесс Octane, свой пользователь базы и Redis ACL только на его ключи. Веб-сервер читает public/ — но никогда ваш .env.", ["свой пользователь", "свой пул", "своя база", "Redis ACL", "поддомены"], "sites-light", "Сайты карточками с поиском"),
  ("Сервисы", "Базы в один клик. Ничего, пока не попросите.", "Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, S3 на RustFS и Reverb. Только localhost, пароли генерируются и хранятся в зашифрованном хранилище. phpMyAdmin и pgAdmin открываются через SSH-туннель, а не в интернет.", ["только localhost", "данные сохраняются", "панели через туннель"], "services-dark", "Сервисы в один клик"),
  ("Деплои и DNS", "Push в GitHub. Направить домен. Готово.", "Привяжите сайт к репозиторию и ветке: Sesame добавит deploy-ключ только на чтение, заберёт код, поставит зависимости, выполнит миграции и перезапустит воркеры. Зоны и записи Cloudflare — рядом с сайтом, с кнопкой «направить на этот сервер».", ["deploy-ключи", "миграции", "DNS-записи", "очистка кэша"], "cloudflare-dark", "DNS-записи в Cloudflare"),
  ("Файлы и терминал", "Когда нужно своими руками.", "Файловый менеджер по SFTP, запертый в папке одного сайта, с небольшим редактором. Настоящий терминал xterm-256color в том же окне.", ["SFTP-клетка", "редактор", "PTY-терминал"], "site-dark", "Сайт с репозиторием и DNS"),
 ],
 "agents_eyebrow": "ИИ-агенты", "agents_h": "Пусть Claude ведёт ваши серверы. На ваших условиях.",
 "agents_p": "Sesame говорит на MCP и REST. Агенты читают всё сразу, а каждое изменение всплывает в приложении и не выполняется, пока вы не нажмёте «Разрешить». Каждый шаг пишется в журнал с именем агента.",
 "surfaces": ["MCP", "REST", "CLI"],
 "approve_who": "Claude Code хочет:", "approve_what": "установить Redis на prod", "approve_d": ["слушает 127.0.0.1:6379", "пароль сгенерирован и сохранён в хранилище"], "approve_deny": "Отклонить", "approve_allow": "Разрешить",
 "nums": [("30", "инструментов для агентов"), ("8", "сервисов в один клик"), ("3", "языка"), ("0", "агентов на сервере")],
 "trust_eyebrow": "Локально по устройству", "trust_h": "Ваши серверы, ваш Mac, ваши ключи.",
 "trust": [
  ("Сначала только чтение", "Обнаружение ничего не пишет и не читает пароли и ключи из .env."),
  ("Зашифрованное хранилище", "XChaCha20-Poly1305 ключом из мастер-пароля через Argon2id."),
  ("Подтвердить, потом выполнить", "План перечисляет шаги. Перед удалением — бэкап и ввод имени."),
  ("Не в командной строке", "Секреты идут через stdin — никогда в argv и никогда в ps."),
  ("Подписанные обновления", "Релизы проверяются подписью ed25519, прежде чем заменить приложение."),
 ],
 "price_eyebrow": "Цены", "price_h": "Бесплатно там, где работает на вашей машине.",
 "plans": [
  ("Self-hosted", "$0", "навсегда", ["Сколько угодно серверов и сайтов", "MCP и REST для агентов", "Cloudflare и GitHub", "Apache-2.0, можно форкать"], "Скачать Sesame", True),
  ("Sesame Cloud", "Позже", "для команд", ["Общие серверы для команды", "Веб- и мобильная панель", "Роли, подтверждения и журнал в одном месте", "Оповещения и бэкапы по расписанию"], "Следить на GitHub", False),
 ],
 "price_micro": "Десктоп-приложение остаётся бесплатным и открытым. Cloud будет платным за хостинг, а не за то, что у вас уже есть.",
 "faq_h": "Вопросы",
 "faq": [
  ("Чем это отличается от Forge или Ploi?", "Это веб-панели, у которых остаётся доступ к вашим серверам. Sesame — приложение на вашем Mac, работает по обычному SSH: ключи остаются у вас, а сервер, настроенный вручную, тоже можно взять под управление."),
  ("А от Coolify?", "Coolify живёт на сервере и собирает контейнеры. Sesame ничего не ставит на сервер и ведёт классический стек Laravel: Caddy или Nginx, PHP-FPM или Octane, supervisor, системные базы."),
  ("Что он меняет на сервере?", "При обнаружении — ничего. Всё остальное — план, который вы видите заранее; необратимые шаги просят ввести имя, перед удалением базы делается бэкап."),
  ("Где хранятся пароли?", "В зашифрованном хранилище на вашем Mac, которое открывается мастер-паролем или файлом-ключом. Никогда в открытом виде в базе и никогда в логах."),
  ("Какие серверы поддерживаются?", "Ubuntu 22.04/24.04 и Debian 12, amd64 и arm64. Обнаружение работает на большинстве Linux-серверов."),
 ],
 "final_h": "Откройте свои серверы.", "final_p": "Попробуйте альфу сначала на тестовом сервере — и расскажите, что он нашёл.",
 "foot_about": "Sesame — нативный менеджер серверов для Laravel-сообщества, удобный для ИИ-агентов, на Rust и GPUI. Он находит по SSH всё, что работает на сервере, и ведёт изолированные сайты, сервисы, базы, деплои и DNS в Cloudflare.",
 "foot_cols": [("Продукт", [("Релизы", REL), ("Изменения", GH + "/blob/main/CHANGELOG.md"), ("Планы", GH + "#roadmap")]), ("Проект", [("GitHub", GH), ("Участие", GH + "/blob/main/CONTRIBUTING.md"), ("Безопасность", GH + "/blob/main/SECURITY.md")])],
 "foot_copy": "Открытый код, Apache-2.0",
},
"az": {
 "dir": "az/", "lang": "az", "locale": "az_AZ",
 "title": "Sesame — serverlərinizi açın. Laravel üçün nativ server meneceri",
 "desc": "Sesame Laravel icması üçün açıq mənbəli nativ server meneceridir. SSH verin: bütün saytları, bazaları və prosesləri tapır, izolə edilmiş saytları, xidmətləri, deployları və Cloudflare DNS-i idarə edir, süni intellekt agentləri isə yalnız sizin icazənizlə işləyir.",
 "nav": ["Necə işləyir", "İmkanlar", "Agentlər", "Təhlükəsizlik", "Qiymət", "Suallar"],
 "download": "Sesame-i yüklə", "github": "GitHub",
 "eyebrow": "Açıq mənbə · Apache-2.0 · " + VERSION,
 "h1": "Serverlərinizi <em>açın.</em>",
 "lead": "Laravel üçün nativ server meneceri. SSH verin — serverdəki bütün saytları, bazaları və prosesləri tapır və onları sizinlə birlikdə idarə edir.",
 "micro": "Pulsuz və açıq mənbə. İndi macOS, sonra Windows. Serverə heç nə quraşdırmaq lazım deyil.",
 "hero_alt": "Sesame: məsləhətlərlə server icmalı",
 "how_eyebrow": "Necə işləyir", "how_h": "SSH girişindən aydın mənzərəyə bir dəqiqədə.",
 "how": [
  ("Birinci dəqiqə", "Qoşulma", "ssh-agent, açar və ya parol. Host açarı known_hosts ilə yoxlanılır. Agent, xidmət və ya açıq port yoxdur."),
  ("İkinci dəqiqə", "Kəşf", "Yalnız oxuyan bir yoxlama Caddy, Nginx və Apache-dəki saytları, Laravel tətbiqlərini, worker-ləri, bazaları, cron-u, sertifikatları və firewall-u tapır."),
  ("Hazır olanda", "İdarəetmə", "Yeni izolə edilmiş saytlar, bir kliklə xidmətlər, bazalar, GitHub-dan deploy və Cloudflare DNS — hər dəyişiklik əvvəlcə göstərilir."),
 ],
 "probe": [("ok", "frankenphp", "3 sayt · Octane"), ("ok", "postgresql 18", "4 baza · 212 MB"), ("ok", "redis", "hər sayta ACL"), ("ok", "supervisor", "növbə ×2 · reverb"), ("warn", "ufw", "söndürülüb"), ("warn", "sertifikat", "shop.example.com · 9 gün")],
 "f": [
  ("İzolə edilmiş saytlar", "Hər sayt öz otağında.", "Hər yeni saytın öz Linux istifadəçisi, öz PHP-FPM pool-u və ya Octane prosesi, öz baza istifadəçisi və yalnız öz açarlarına çıxışı olan Redis ACL-i var. Veb server public/ qovluğunu oxuyur — .env-i heç vaxt.", ["öz istifadəçi", "öz pool", "öz baza", "Redis ACL", "subdomenlər"], "sites-light", "Axtarışlı sayt kartları"),
  ("Xidmətlər", "Bazalar bir kliklə. Siz istəməyincə heç nə.", "Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, RustFS üzərində S3 və Reverb. Yalnız localhost, parollar yaradılır və şifrəli anbarda saxlanılır. phpMyAdmin və pgAdmin SSH tuneli ilə açılır, internetə yox.", ["yalnız localhost", "məlumat saxlanılır", "panellər tunel ilə"], "services-dark", "Bir kliklə xidmətlər"),
  ("Deploy və DNS", "GitHub-a push. Domeni yönləndir. Hazır.", "Saytı repozitoriya və branch-a bağlayın: Sesame yalnız oxuma deploy açarı əlavə edir, kodu çəkir, asılılıqları qurur, miqrasiyaları icra edir və worker-ləri yenidən başladır. Cloudflare zonaları və qeydləri saytın yanındadır, «bu serverə yönləndir» düyməsi ilə.", ["deploy açarları", "miqrasiyalar", "DNS qeydləri", "keşin təmizlənməsi"], "cloudflare-dark", "Cloudflare DNS qeydləri"),
  ("Fayllar və terminal", "Öz əlinizlə lazım olanda.", "Bir saytın qovluğuna bağlanmış SFTP fayl meneceri və kiçik redaktor. Eyni pəncərədə əsl xterm-256color terminal.", ["SFTP qəfəsi", "redaktor", "PTY terminal"], "site-dark", "Repozitoriya və DNS ilə sayt"),
 ],
 "agents_eyebrow": "Süni intellekt agentləri", "agents_h": "Qoy Claude serverlərinizi idarə etsin. Sizin şərtlərinizlə.",
 "agents_p": "Sesame MCP və REST dilində danışır. Agentlər hər şeyi dərhal oxuyur, hər dəyişiklik isə tətbiqdə görünür və siz «İcazə ver» basmayınca icra olunmur. Hər addım agentin adı ilə jurnala yazılır.",
 "surfaces": ["MCP", "REST", "CLI"],
 "approve_who": "Claude Code istəyir:", "approve_what": "prod-a Redis quraşdırmaq", "approve_d": ["127.0.0.1:6379-da dinləyir", "parol yaradılıb və anbarda saxlanılıb"], "approve_deny": "Rədd et", "approve_allow": "İcazə ver",
 "nums": [("30", "agent aləti"), ("8", "bir kliklə xidmət"), ("3", "dil"), ("0", "serverdə agent")],
 "trust_eyebrow": "Lokal quruluş", "trust_h": "Sizin serverlər, sizin Mac, sizin açarlar.",
 "trust": [
  ("Əvvəlcə yalnız oxuma", "Kəşf heç nə yazmır və .env-dən parol və açar oxumur."),
  ("Şifrəli anbar", "Master paroldan Argon2id ilə alınan açarla XChaCha20-Poly1305."),
  ("Təsdiqlə, sonra icra et", "Plan addımları göstərir. Silmədən əvvəl ehtiyat nüsxə və adın yazılması."),
  ("Komanda sətrindən kənar", "Sirlər stdin ilə ötürülür — heç vaxt argv-də, heç vaxt ps-də."),
  ("İmzalı yeniləmələr", "Buraxılışlar tətbiqi əvəz etməzdən əvvəl ed25519 imzası ilə yoxlanılır."),
 ],
 "price_eyebrow": "Qiymət", "price_h": "Sizin maşınınızda işləyəndə pulsuzdur.",
 "plans": [
  ("Self-hosted", "$0", "həmişəlik", ["Limitsiz server və sayt", "Agentlər üçün MCP və REST", "Cloudflare və GitHub", "Apache-2.0, fork edə bilərsiniz"], "Sesame-i yüklə", True),
  ("Sesame Cloud", "Sonra", "komandalar üçün", ["Komanda ilə ortaq serverlər", "Veb və mobil panel", "Rollar, təsdiqlər və jurnal bir yerdə", "Bildirişlər və planlı ehtiyat nüsxələr"], "GitHub-da izlə", False),
 ],
 "price_micro": "Masaüstü tətbiq pulsuz və açıq mənbə olaraq qalır. Cloud hostinq üçün ödənişli olacaq, artıq sizdə olan imkanlar üçün yox.",
 "faq_h": "Suallar",
 "faq": [
  ("Forge və ya Ploi-dən fərqi nədir?", "Onlar serverlərinizə çıxışı saxlayan veb panellərdir. Sesame Mac-inizdə işləyən tətbiqdir və adi SSH ilə danışır: açarlar sizdə qalır, əl ilə qurulmuş serveri də idarəyə götürmək olar."),
  ("Bəs Coolify-dan?", "Coolify serverdə işləyir və konteynerlər qurur. Sesame serverə heç nə quraşdırmır və klassik Laravel stekini idarə edir: Caddy və ya Nginx, PHP-FPM və ya Octane, supervisor, sistem bazaları."),
  ("Serverdə nəyi dəyişir?", "Kəşf zamanı heç nəyi. Qalan hər şey əvvəlcədən gördüyünüz plandır; geri qaytarılmaz addımlar adı yazmağı tələb edir, bazanı silməzdən əvvəl ehtiyat nüsxə çıxarılır."),
  ("Parollar harada saxlanılır?", "Mac-inizdəki şifrəli anbarda, master parol və ya açar faylı ilə açılır. Heç vaxt açıq şəkildə bazada və ya loglarda deyil."),
  ("Hansı serverlər dəstəklənir?", "Ubuntu 22.04/24.04 və Debian 12, amd64 və arm64. Kəşf əksər Linux serverlərində işləyir."),
 ],
 "final_h": "Serverlərinizi açın.", "final_p": "Alfa versiyanı əvvəlcə test serverində sınayın — və nə tapdığını bizə deyin.",
 "foot_about": "Sesame Laravel icması üçün süni intellektə uyğun nativ server meneceridir, Rust və GPUI ilə yazılıb. SSH ilə serverdə işləyən hər şeyi tapır və izolə edilmiş saytları, xidmətləri, bazaları, deployları və Cloudflare DNS-i idarə edir.",
 "foot_cols": [("Məhsul", [("Buraxılışlar", REL), ("Dəyişikliklər", GH + "/blob/main/CHANGELOG.md"), ("Yol xəritəsi", GH + "#roadmap")]), ("Layihə", [("GitHub", GH), ("İştirak", GH + "/blob/main/CONTRIBUTING.md"), ("Təhlükəsizlik", GH + "/blob/main/SECURITY.md")])],
 "foot_copy": "Açıq mənbə, Apache-2.0",
},
}

CHECK = '<svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12 4.5 4.5L19 7"/></svg>'
DOWN = '<svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v12m0 0-5-5m5 5 5-5M5 20h14"/></svg>'

SURFACES = {
 "MCP": '''<span class="c"># Claude Code</span>
claude mcp add sesame -- sesame mcp

<span class="c"># Claude Desktop, Cursor</span>
{ "mcpServers": { "sesame": {
    "command": "sesame", "args": ["mcp"] } } }''',
 "REST": '''sesame api-token        <span class="c"># shown once</span>
sesame serve            <span class="c"># 127.0.0.1:7878</span>

curl -X POST localhost:7878/v1/tools/list_sites \\
  -H "Authorization: Bearer $TOKEN" \\
  -d '{"server":"prod"}\'''',
 "CLI": '''sesame add prod root@203.0.113.10
sesame discover prod
sesame do prod new-site shop.example.com \\
  --database postgres --redis --queue
sesame do prod install redis''',
}


def page(code):
    t = T[code]
    up = "../" if t["dir"] else ""
    shot = lambda name: f"{up}assets/screens/{name}-{code}.webp"
    alts = "\n".join(f'  <link rel="alternate" hreflang="{c}" href="{SITE}/{T[c]["dir"]}">' for c in T)
    cur = ' aria-current="page"'
    langs = "".join(
        f'<a href="{(up + T[c]["dir"]) or "./"}" lang="{c}" hreflang="{c}"{cur if c == code else ""}>{c.upper()}</a>'
        for c in ["en", "az", "ru"])
    ids = ["how", "features", "agents", "security", "pricing", "faq"]
    nav = "".join(f'<a href="#{i}">{e(n)}</a>' for i, n in zip(ids, t["nav"]))
    how = "".join(f'''
        <div class="step reveal">
          <span class="mono k">{i + 1:02d} · {e(w)}</span>
          <h3>{e(h)}</h3>
          <p>{e(p)}</p>
        </div>''' for i, (w, h, p) in enumerate(t["how"]))
    probe = "".join(f'<li class="reveal"><span class="dot {s}"></span><span class="mono">{e(a)}</span><span class="dim">{e(b)}</span></li>' for s, a, b in t["probe"])
    feats = ""
    for i, (eb, h, p, chips, img, alt) in enumerate(t["f"]):
        chips_h = "".join(f"<li>{e(c)}</li>" for c in chips)
        feats += f'''
    <section class="row{' flip' if i % 2 else ''}">
      <div class="frame feature">
        <div class="copy reveal">
          <span class="eyebrow">{e(eb)}</span>
          <h2>{e(h)}</h2>
          <p>{e(p)}</p>
          <ul class="chips">{chips_h}</ul>
        </div>
        <figure class="shot reveal"><img src="{shot(img)}" width="1600" height="1018" loading="lazy" decoding="async" alt="{e(alt)}"></figure>
      </div>
    </section>'''
    tabs = "".join(f'<button type="button" role="tab" class="tab" aria-selected="{"true" if i == 0 else "false"}" data-tab="{s}">{s}</button>' for i, s in enumerate(t["surfaces"]))
    panes = "".join(f'<pre class="pane" data-pane="{s}"{"" if i == 0 else " hidden"}><code>{SURFACES[s]}</code></pre>' for i, s in enumerate(t["surfaces"]))
    approve_d = "".join(f"<li>{e(d)}</li>" for d in t["approve_d"])
    nums = "".join(f'<div class="num reveal"><b>{n}</b><span>{e(l)}</span></div>' for n, l in t["nums"])
    trust = "".join(f'<div class="cell reveal"><span class="mono k">{i + 1:02d}</span><h3>{e(h)}</h3><p>{e(p)}</p></div>' for i, (h, p) in enumerate(t["trust"]))
    plans = ""
    for name, price, per, items, cta, primary in t["plans"]:
        li = "".join(f"<li>{CHECK}{e(x)}</li>" for x in items)
        href = REL if primary else GH
        plans += f'''
        <div class="plan{' main' if primary else ''} reveal">
          <h3>{e(name)}</h3>
          <p class="price"><b>{e(price)}</b><span>{e(per)}</span></p>
          <ul>{li}</ul>
          <a class="btn{'' if primary else ' ghost'}" href="{href}">{e(cta)}</a>
        </div>'''
    faq = "".join(f'<details class="reveal"><summary>{e(q)}</summary><p>{e(a)}</p></details>' for q, a in t["faq"])
    cols = "".join(
        f'<div><h4>{e(h)}</h4>' + "".join(f'<a href="{u}">{e(n)}</a>' for n, u in links) + '</div>'
        for h, links in t["foot_cols"])
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
  <link rel="icon" href="{up}assets/logo.svg" type="image/svg+xml">
  <meta name="theme-color" content="#B33A16">
  <meta property="og:type" content="website">
  <meta property="og:title" content="{e(t['title'])}">
  <meta property="og:description" content="{e(t['desc'])}">
  <meta property="og:url" content="{SITE}/{t['dir']}">
  <meta property="og:image" content="{SITE}/assets/og.jpg">
  <meta property="og:locale" content="{t['locale']}">
  <meta name="twitter:card" content="summary_large_image">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Inter+Tight:wght@300;400;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
  <link rel="stylesheet" href="{up}assets/site.css?v={CSS_V}">
  <script defer src="{up}assets/site.js?v={JS_V}"></script>
</head>
<body>
  <a class="skip" href="#main">Skip</a>
  <header class="nav">
    <div class="frame bar">
      <a class="brand" href="{(up + t['dir']) or './'}"><img src="{up}assets/logo.svg" width="26" height="26" alt="">Sesame</a>
      <nav class="links" aria-label="Sections">{nav}</nav>
      <div class="right">
        <div class="langs" aria-label="Language">{langs}</div>
        <a class="btn sm" href="{REL}">{DOWN}{e(t['download'])}</a>
        <button class="burger" type="button" aria-label="Menu" aria-expanded="false"><span></span><span></span></button>
      </div>
    </div>
  </header>

  <main id="main">
    <section class="hero">
      <div class="frame">
        <div class="hero-copy">
          <span class="pill">{e(t['eyebrow'])}</span>
          <h1>{t['h1']}</h1>
          <p class="lead">{e(t['lead'])}</p>
          <div class="ctas">
            <a class="btn light" href="{REL}">{DOWN}{e(t['download'])}</a>
            <a class="btn outline" href="{GH}">{e(t['github'])} →</a>
          </div>
          <p class="micro">{e(t['micro'])}</p>
        </div>
        <figure class="product"><img src="{shot('overview-dark')}" width="1600" height="1018" fetchpriority="high" alt="{e(t['hero_alt'])}"></figure>
      </div>
    </section>

    <section id="how" class="row">
      <div class="frame">
        <div class="head reveal">
          <span class="eyebrow">{e(t['how_eyebrow'])}</span>
          <h2>{e(t['how_h'])}</h2>
        </div>
        <div class="steps">{how}
        </div>
        <div class="probe reveal" aria-hidden="true">
          <div class="probe-top"><span class="mono">sesame discover prod</span><span class="dot ok"></span></div>
          <ul>{probe}</ul>
        </div>
      </div>
    </section>

    <div id="features"></div>{feats}

    <section id="agents" class="row">
      <div class="frame split">
        <div class="copy reveal">
          <span class="eyebrow">{e(t['agents_eyebrow'])}</span>
          <h2>{e(t['agents_h'])}</h2>
          <p>{e(t['agents_p'])}</p>
          <div class="approve" aria-hidden="true">
            <div class="ap-top"><span class="dot ok"></span><b>{e(t['approve_who'])}</b><span class="tagc">change</span></div>
            <p class="ap-what">{e(t['approve_what'])}</p>
            <ul>{approve_d}</ul>
            <div class="ap-btns"><span class="btn ghost sm">{e(t['approve_deny'])}</span><span class="btn sm">{e(t['approve_allow'])}</span></div>
          </div>
        </div>
        <div class="code reveal">
          <div class="tabs" role="tablist">{tabs}</div>
          {panes}
        </div>
      </div>
      <div class="frame nums">{nums}</div>
    </section>

    <section id="security" class="row">
      <div class="frame">
        <div class="head reveal">
          <span class="eyebrow">{e(t['trust_eyebrow'])}</span>
          <h2>{e(t['trust_h'])}</h2>
        </div>
        <div class="cells">{trust}</div>
      </div>
    </section>

    <section id="pricing" class="row">
      <div class="frame">
        <div class="head reveal">
          <span class="eyebrow">{e(t['price_eyebrow'])}</span>
          <h2>{e(t['price_h'])}</h2>
        </div>
        <div class="plans">{plans}
        </div>
        <p class="micro center">{e(t['price_micro'])}</p>
      </div>
    </section>

    <section id="faq" class="row">
      <div class="frame narrow">
        <h2 class="reveal">{e(t['faq_h'])}</h2>
        <div class="faq">{faq}</div>
      </div>
    </section>

    <section class="final">
      <div class="frame">
        <h2>{e(t['final_h'])}</h2>
        <p>{e(t['final_p'])}</p>
        <a class="btn light" href="{REL}">{DOWN}{e(t['download'])}</a>
      </div>
    </section>
  </main>

  <footer class="foot">
    <div class="frame cols">
      <div class="about">
        <a class="brand" href="{(up + t['dir']) or './'}"><img src="{up}assets/logo.svg" width="22" height="22" alt="">Sesame</a>
        <p>{e(t['foot_about'])}</p>
      </div>
      {cols}
    </div>
    <div class="frame copy-line"><span>© 2026 Sesame · {e(t['foot_copy'])}</span><span class="langs">{langs}</span></div>
  </footer>
</body>
</html>
'''


for code, t in T.items():
    d = os.path.join(OUT, t["dir"])
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "index.html"), "w", encoding="utf-8") as f:
        f.write(page(code))
    print("wrote", os.path.join(t["dir"], "index.html"))
