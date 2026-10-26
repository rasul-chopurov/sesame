// Shared state: servers, their snapshots, integrations and approvals.
import { reactive } from 'vue'
import { api, tool } from './api.js'

export const state = reactive({
  servers: [],
  snaps: {},
  services: {},
  links: {},
  integrations: { github: null, cloudflare: null, resend: null },
  zones: [],
  records: {},
  approvals: 0,
  errors: {},
  loading: {},
  theme: read('sesame.theme') || 'system',
})

function read(k) {
  try { return localStorage.getItem(k) } catch { return null }
}

export function setTheme(v) {
  state.theme = v
  try { localStorage.setItem('sesame.theme', v) } catch { /* ignore */ }
  applyTheme()
}

export function applyTheme() {
  const dark = state.theme === 'dark' || (state.theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches)
  document.documentElement.dataset.theme = dark ? 'dark' : 'light'
}

export async function loadServers() {
  state.servers = await tool('list_servers')
  await Promise.all(state.servers.filter((s) => s.discovered_at).map((s) => loadServer(s.name)))
}

export async function loadServer(name) {
  state.loading[name] = true
  try {
    const [snap, services, links] = await Promise.all([
      tool('get_snapshot', { server: name }),
      tool('list_services', { server: name }).catch(() => []),
      tool('site_links', { server: name }).catch(() => ({})),
    ])
    state.snaps[name] = snap
    state.services[name] = services
    state.links[name] = links
    delete state.errors[name]
  } catch (e) {
    state.errors[name] = e.message
  } finally {
    state.loading[name] = false
  }
}

export async function rediscover(name) {
  state.loading[name] = true
  try {
    await tool('discover_server', { server: name })
    await loadServer(name)
    state.servers = await tool('list_servers')
  } catch (e) {
    state.errors[name] = e.message
    state.loading[name] = false
  }
}

export async function loadIntegrations() {
  try { state.integrations = await tool('integrations') } catch { /* not allowed or offline */ }
  if (state.integrations.cloudflare?.connected) await loadCloudflare()
}

export async function loadCloudflare() {
  try {
    state.zones = await tool('cloudflare_zones')
    const all = await Promise.all(state.zones.map((z) => tool('cloudflare_records', { zone: z.name }).catch(() => [])))
    state.zones.forEach((z, i) => { state.records[z.name] = all[i] })
  } catch { /* ignore */ }
}

export async function pollApprovals() {
  try { state.approvals = (await api('/approvals')).length } catch { /* ignore */ }
}

/** The server's public IP: its host when it is an address. */
export function serverIp(name) {
  const s = state.servers.find((x) => x.name === name)
  const host = s?.address?.split('@').pop().split(':')[0] || ''
  return /^\d+\.\d+\.\d+\.\d+$/.test(host) ? host : ''
}

export function zoneFor(host) {
  return state.zones.filter((z) => host === z.name || host.endsWith(`.${z.name}`)).sort((a, b) => b.name.length - a.name.length)[0]
}

/** DNS state of a host for a server, like the desktop's DnsStatus. */
export function dnsStatus(host, ip) {
  if (!state.integrations.cloudflare?.connected) return null
  const zone = zoneFor(host)
  if (!zone) return { kind: 'no_zone' }
  const recs = (state.records[zone.name] || []).filter((r) => r.name === host && ['A', 'AAAA', 'CNAME'].includes(r.type))
  if (!recs.length) return { kind: 'missing', zone: zone.name }
  const ok = recs.find((r) => r.type === 'A' && r.content === ip)
  return ok ? { kind: 'ok', record: ok } : { kind: 'elsewhere', record: recs[0], zone: zone.name }
}
