import { ref } from 'vue'

const dict = {
  en: {
    servers: 'Servers', approvals: 'Approvals', audit: 'Audit log', team: 'Team', signout: 'Sign out',
    signin: 'Sign in', email: 'Email', password: 'Password', name: 'Name',
    setup_title: 'Set up Sesame Cloud', setup_code: 'Setup code (in the server log)', create_owner: 'Create owner account',
    overview: 'Overview', sites: 'Sites', services: 'Services', databases: 'Databases', rediscover: 'Rediscover',
    attention: 'Needs attention', all_good: 'Nothing needs attention.', discovered: 'discovered',
    install: 'Install', uninstall: 'Uninstall', installed: 'installed', not_installed: 'not installed',
    create_db: 'Create database', drop: 'Drop', engine: 'Engine', user: 'User',
    maintenance_on: 'Maintenance on', maintenance_off: 'Maintenance off', site_off: 'Turn off', site_on: 'Turn on', deploy: 'Deploy',
    confirm: 'Confirm', cancel: 'Cancel', type_to_confirm: 'Type {word} to confirm', running: 'Running…',
    waiting_approval: 'Waiting for an admin to approve…', done: 'Done', allow: 'Allow', deny: 'Deny', nothing_pending: 'Nothing waiting.',
    role: 'Role', add_person: 'Add a person', add_agent: 'Add an agent', remove: 'Remove', token_once: 'Copy this token now — it is shown once.',
    when: 'When', who: 'Who', what: 'What', server: 'Server', ok: 'OK', failed: 'failed', disabled: 'off', owner: 'owner',
    no_servers: 'No servers yet. Add one in the Sesame app or with `sesame add` on this machine.',
    changes: 'Changes the server', destructive: 'Cannot be undone', secrets_hidden: 'Generated passwords are kept in the vault.',
  },
  ru: {
    servers: 'Серверы', approvals: 'Подтверждения', audit: 'Журнал', team: 'Команда', signout: 'Выйти',
    signin: 'Войти', email: 'Email', password: 'Пароль', name: 'Имя',
    setup_title: 'Настройка Sesame Cloud', setup_code: 'Код настройки (в логе сервера)', create_owner: 'Создать владельца',
    overview: 'Обзор', sites: 'Сайты', services: 'Сервисы', databases: 'Базы данных', rediscover: 'Обновить',
    attention: 'Требует внимания', all_good: 'Всё в порядке.', discovered: 'обнаружено',
    install: 'Установить', uninstall: 'Удалить', installed: 'установлен', not_installed: 'не установлен',
    create_db: 'Создать базу', drop: 'Удалить', engine: 'Движок', user: 'Пользователь',
    maintenance_on: 'Обслуживание вкл', maintenance_off: 'Обслуживание выкл', site_off: 'Отключить', site_on: 'Включить', deploy: 'Деплой',
    confirm: 'Подтвердить', cancel: 'Отмена', type_to_confirm: 'Введите {word} для подтверждения', running: 'Выполняется…',
    waiting_approval: 'Ждём подтверждения администратора…', done: 'Готово', allow: 'Разрешить', deny: 'Отклонить', nothing_pending: 'Ничего не ждёт.',
    role: 'Роль', add_person: 'Добавить человека', add_agent: 'Добавить агента', remove: 'Убрать', token_once: 'Скопируйте токен сейчас — он показывается один раз.',
    when: 'Когда', who: 'Кто', what: 'Что', server: 'Сервер', ok: 'OK', failed: 'ошибка', disabled: 'выкл', owner: 'владелец',
    no_servers: 'Серверов пока нет. Добавьте в приложении Sesame или командой `sesame add` на этой машине.',
    changes: 'Меняет сервер', destructive: 'Необратимо', secrets_hidden: 'Сгенерированные пароли сохранены в хранилище.',
  },
  az: {
    servers: 'Serverlər', approvals: 'Təsdiqlər', audit: 'Jurnal', team: 'Komanda', signout: 'Çıxış',
    signin: 'Daxil ol', email: 'Email', password: 'Parol', name: 'Ad',
    setup_title: 'Sesame Cloud quraşdırması', setup_code: 'Quraşdırma kodu (server logunda)', create_owner: 'Sahib hesabı yarat',
    overview: 'İcmal', sites: 'Saytlar', services: 'Xidmətlər', databases: 'Bazalar', rediscover: 'Yenilə',
    attention: 'Diqqət tələb edir', all_good: 'Hər şey qaydasındadır.', discovered: 'kəşf edilib',
    install: 'Quraşdır', uninstall: 'Sil', installed: 'quraşdırılıb', not_installed: 'quraşdırılmayıb',
    create_db: 'Baza yarat', drop: 'Sil', engine: 'Mühərrik', user: 'İstifadəçi',
    maintenance_on: 'Texniki rejim aç', maintenance_off: 'Texniki rejim bağla', site_off: 'Söndür', site_on: 'Yandır', deploy: 'Deploy',
    confirm: 'Təsdiqlə', cancel: 'Ləğv et', type_to_confirm: 'Təsdiq üçün {word} yazın', running: 'İcra olunur…',
    waiting_approval: 'Admin təsdiqi gözlənilir…', done: 'Hazırdır', allow: 'İcazə ver', deny: 'Rədd et', nothing_pending: 'Gözləyən yoxdur.',
    role: 'Rol', add_person: 'İnsan əlavə et', add_agent: 'Agent əlavə et', remove: 'Çıxar', token_once: 'Tokeni indi kopyalayın — bir dəfə göstərilir.',
    when: 'Nə vaxt', who: 'Kim', what: 'Nə', server: 'Server', ok: 'OK', failed: 'xəta', disabled: 'sönük', owner: 'sahib',
    no_servers: 'Hələ server yoxdur. Sesame tətbiqində və ya bu maşında `sesame add` ilə əlavə edin.',
    changes: 'Serveri dəyişir', destructive: 'Geri qaytarılmaz', secrets_hidden: 'Yaradılan parollar anbarda saxlanılır.',
  },
}

function initial() {
  try {
    const saved = localStorage.getItem('sesame.lang')
    if (saved && dict[saved]) return saved
  } catch { /* ignore */ }
  const l = (navigator.language || 'en').slice(0, 2)
  return dict[l] ? l : 'en'
}

export const lang = ref(initial())
export const langs = Object.keys(dict)

export function setLang(l) {
  lang.value = l
  try { localStorage.setItem('sesame.lang', l) } catch { /* ignore */ }
}

export function t(key, params = {}) {
  let s = dict[lang.value]?.[key] ?? dict.en[key] ?? key
  for (const [k, v] of Object.entries(params)) s = s.replace(`{${k}}`, v)
  return s
}
