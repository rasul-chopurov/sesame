// The desktop's server sections, in its order.
export const SECTIONS = [
  { slug: 'overview', key: 'nav.overview', icon: 'gauge' },
  { slug: 'sites', key: 'nav.sites', icon: 'globe', count: (s) => s.sites.filter((x) => x.domains.join() !== '*').length },
  { slug: 'processes', key: 'nav.processes', icon: 'activity', count: (s) => s.supervisor.length },
  { slug: 'services', key: 'nav.services', icon: 'package' },
  { slug: 'databases', key: 'nav.databases', icon: 'database', count: (s) => s.databases.reduce((n, d) => n + d.databases.length, 0) },
  { slug: 'mail-cron', key: 'nav.mail_cron', icon: 'mail' },
  { slug: 'security', key: 'nav.security', icon: 'shield' },
  { slug: 'terminal', key: 'nav.terminal', icon: 'terminal' },
]

export const DB = {
  my_sql: { name: 'MySQL', logo: 'mysql', tool: 'mysql' },
  maria_db: { name: 'MariaDB', logo: 'mariadb', tool: 'mariadb' },
  postgres: { name: 'PostgreSQL', logo: 'postgresql', tool: 'postgres' },
  redis: { name: 'Redis', logo: 'redis', tool: null },
  mongo_db: { name: 'MongoDB', logo: 'mongodb', tool: 'mongodb' },
}

export const FRAMEWORK = { laravel: 'Laravel', word_press: 'WordPress', wordpress: 'WordPress', node: 'Node.js', static: 'Static' }

export function frameworkLabel(site) {
  const n = FRAMEWORK[site.framework]
  if (!n) return ''
  return site.framework_version ? `${n} ${site.framework_version}` : n
}

export function siteTitle(site, t) {
  return site.domains.join() === '*' ? t('sites.any_host') : site.domains[0] || ''
}

export function bytes(n) {
  if (!n && n !== 0) return '—'
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++ }
  return `${n.toFixed(i ? 1 : 0)} ${u[i]}`
}

export function duration(s) {
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60)
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`
}

export const short = (t) => (t || '').replace('T', ' ').slice(0, 16)
export const canChange = (u) => u && u.role !== 'viewer'
export const isAdmin = (u) => u && ['admin', 'owner'].includes(u.role) && !u.agent
