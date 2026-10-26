// The desktop app's strings (locales.json, generated from app.yml) plus the
// few that only the web app needs.
import { ref } from 'vue'
import desktop from './locales.json'

const web = {
  en: {
    'web.approvals': 'Approvals', 'web.audit': 'Audit log', 'web.team': 'Team', 'web.signout': 'Sign out', 'web.signin': 'Sign in',
    'web.email': 'Email', 'web.password': 'Password', 'web.name': 'Name', 'web.setup_title': 'Set up Sesame Cloud',
    'web.setup_code': 'Setup code (in the server log)', 'web.create_owner': 'Create owner account', 'web.waiting_approval': 'Waiting for an admin to approve…',
    'web.nothing_pending': 'Nothing waiting.', 'web.role': 'Role', 'web.add_person': 'Add a person', 'web.add_agent': 'Add an agent',
    'web.remove': 'Remove', 'web.token_once': 'Copy this token now — it is shown once.', 'web.when': 'When', 'web.who': 'Who', 'web.what': 'What',
    'web.server': 'Server', 'web.ok': 'OK', 'web.failed': 'failed', 'web.panel_desktop': 'Opens through an SSH tunnel from the desktop app.',
    'web.agents_lead': 'Agents call the same API with their own token; every change they ask for waits here for a person.',
    'web.mcp_url': 'REST for agents', 'web.no_servers': 'No servers yet.', 'web.read_only': 'Your role can only read.',
    'web.offline': 'Offline — showing the last loaded data.',
  },
  ru: {
    'web.approvals': 'Подтверждения', 'web.audit': 'Журнал', 'web.team': 'Команда', 'web.signout': 'Выйти', 'web.signin': 'Войти',
    'web.email': 'Email', 'web.password': 'Пароль', 'web.name': 'Имя', 'web.setup_title': 'Настройка Sesame Cloud',
    'web.setup_code': 'Код настройки (в логе сервера)', 'web.create_owner': 'Создать владельца', 'web.waiting_approval': 'Ждём подтверждения администратора…',
    'web.nothing_pending': 'Ничего не ждёт.', 'web.role': 'Роль', 'web.add_person': 'Добавить человека', 'web.add_agent': 'Добавить агента',
    'web.remove': 'Убрать', 'web.token_once': 'Скопируйте токен сейчас — он показывается один раз.', 'web.when': 'Когда', 'web.who': 'Кто', 'web.what': 'Что',
    'web.server': 'Сервер', 'web.ok': 'OK', 'web.failed': 'ошибка', 'web.panel_desktop': 'Открывается через SSH-туннель из десктоп-приложения.',
    'web.agents_lead': 'Агенты работают через тот же API со своим токеном; каждое их изменение ждёт здесь решения человека.',
    'web.mcp_url': 'REST для агентов', 'web.no_servers': 'Серверов пока нет.', 'web.read_only': 'Ваша роль позволяет только смотреть.',
    'web.offline': 'Нет связи — показаны последние данные.',
  },
  az: {
    'web.approvals': 'Təsdiqlər', 'web.audit': 'Jurnal', 'web.team': 'Komanda', 'web.signout': 'Çıxış', 'web.signin': 'Daxil ol',
    'web.email': 'Email', 'web.password': 'Parol', 'web.name': 'Ad', 'web.setup_title': 'Sesame Cloud quraşdırması',
    'web.setup_code': 'Quraşdırma kodu (server logunda)', 'web.create_owner': 'Sahib hesabı yarat', 'web.waiting_approval': 'Admin təsdiqi gözlənilir…',
    'web.nothing_pending': 'Gözləyən yoxdur.', 'web.role': 'Rol', 'web.add_person': 'İnsan əlavə et', 'web.add_agent': 'Agent əlavə et',
    'web.remove': 'Çıxar', 'web.token_once': 'Tokeni indi kopyalayın — bir dəfə göstərilir.', 'web.when': 'Nə vaxt', 'web.who': 'Kim', 'web.what': 'Nə',
    'web.server': 'Server', 'web.ok': 'OK', 'web.failed': 'xəta', 'web.panel_desktop': 'Masaüstü tətbiqdən SSH tuneli ilə açılır.',
    'web.agents_lead': 'Agentlər öz tokeni ilə eyni API-dən istifadə edir; istədikləri hər dəyişiklik burada insan qərarını gözləyir.',
    'web.mcp_url': 'Agentlər üçün REST', 'web.no_servers': 'Hələ server yoxdur.', 'web.read_only': 'Rolunuz yalnız baxmağa icazə verir.',
    'web.offline': 'Əlaqə yoxdur — son məlumatlar göstərilir.',
  },
}

function initial() {
  try {
    const saved = localStorage.getItem('sesame.lang')
    if (saved && web[saved]) return saved
  } catch { /* private mode */ }
  const l = (navigator.language || 'en').slice(0, 2)
  return web[l] ? l : 'en'
}

export const lang = ref(initial())
export const langs = ['en', 'ru', 'az']

export function setLang(l) {
  lang.value = l
  document.documentElement.lang = l
  try { localStorage.setItem('sesame.lang', l) } catch { /* ignore */ }
}

/** `t('overview.of', { used, total })` with the desktop's `%{name}` placeholders. */
export function t(key, params = {}) {
  const l = lang.value
  let s = desktop[l]?.[key] ?? web[l]?.[key] ?? desktop.en[key] ?? web.en[key] ?? key
  for (const [k, v] of Object.entries(params)) s = s.replaceAll(`%{${k}}`, String(v))
  return s
}

/** Insight text in the current language (codes and params come from discovery). */
export function insight(i) {
  const params = Object.fromEntries(i.params || [])
  const key = `insight.${i.code}`
  return t(key) === key ? i.message : t(key, params)
}
