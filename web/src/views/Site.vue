<script setup>
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { session, tool } from '../api.js'
import { t } from '../i18n.js'
import { state, rediscover, dnsStatus, serverIp, zoneFor, loadCloudflare, loadServer } from '../store.js'
import { ask } from '../runner.js'
import { frameworkLabel, siteTitle, canChange } from '../sections.js'
import Icon from '../components/Icon.vue'
import Brand from '../components/Brand.vue'

const props = defineProps({ name: String, domain: String })
const router = useRouter()
const snap = computed(() => state.snaps[props.name])
const site = computed(() => snap.value?.sites.find((s) => s.domains.includes(props.domain)))
const can = computed(() => canChange(session.user))
const laravel = computed(() => site.value?.framework === 'laravel')
const managed = computed(() => (site.value?.app_path || '').startsWith('/home/s_'))
const procs = computed(() => snap.value.supervisor.filter((p) => site.value.processes.includes(p.name)))
const env = computed(() => site.value?.env || {})
const refresh = () => rediscover(props.name)
const base = computed(() => ({ server: props.name, domain: props.domain }))

// ---- header actions ----
const maintenance = (on) => ask('site_maintenance', { ...base.value, on }, { title: on ? t('sites.maintenance_on') : t('sites.maintenance_off'), after: refresh })
const reverb = () => ask('site_reverb', base.value, { title: `${t('sites.enable_reverb')}: ${props.domain}`, after: refresh })
const toggle = () => {
  const on = site.value.disabled
  ask('site_enabled', { ...base.value, enabled: on }, { title: on ? t('sites.enable') : t('sites.disable'), risk: on ? 'change' : 'destructive', word: on ? '' : props.domain, after: refresh })
}

// ---- site services (the desktop's site_services_card) ----
const dbEngine = computed(() => snap.value.databases.find((d) => ['my_sql', 'maria_db', 'postgres'].includes(d.kind)))
const hasRedis = computed(() => snap.value.databases.some((d) => d.kind === 'redis'))
const workers = computed(() => procs.value.filter((p) => p.role === 'queue').length)
const hasReverb = computed(() => procs.value.some((p) => p.role === 'reverb'))
const scheduled = computed(() => snap.value.cron.some((e) => e.command.includes('schedule:run') && e.command.includes(site.value.app_path)))
const ENGINE = { my_sql: 'mysql', maria_db: 'mariadb', postgres: 'postgres' }
const addDb = () => ask('site_database', { ...base.value, engine: ENGINE[dbEngine.value.kind] }, { title: `${t('ns.database')} · ${props.domain}`, after: refresh })
const addRedis = () => ask('site_redis', base.value, { title: `Redis · ${props.domain}`, after: refresh })
const addQueue = () => ask('site_queue', { ...base.value, connection: env.value.redis_username || env.value.queue_connection === 'redis' ? 'redis' : 'database' }, { title: `${t('ns.queue')} · ${props.domain}`, after: refresh })
const addScheduler = () => ask('site_scheduler', base.value, { title: `${t('ns.scheduler')} · ${props.domain}`, after: refresh })

// ---- subdomains ----
const sub = ref('')
const subs = computed(() => snap.value.sites.filter((s) => s.domains.some((d) => d.endsWith(`.${props.domain}`) && !site.value.domains.includes(d))))
const addSub = () => sub.value.trim() && router.push({ path: `/servers/${encodeURIComponent(props.name)}/new-site`, query: { domain: `${sub.value.trim().toLowerCase()}.${props.domain}` } })

// ---- repository ----
const link = computed(() => state.links[props.name]?.[props.domain])
const head = ref(null)
const repos = ref(null)
const repoQ = ref('')
const branches = ref({})
const ghConnected = computed(() => state.integrations.github?.connected)
async function checkHead() { try { head.value = await tool('github_head', { repo: link.value.repo, branch: link.value.branch }) } catch (e) { head.value = { error: e.message } } }
async function loadRepos() { try { repos.value = await tool('github_repos') } catch { repos.value = [] } }
async function loadBranches(r) { try { branches.value = { ...branches.value, [r]: (await tool('github_branches', { repo: r })).map((b) => b.name) } } catch { /* ignore */ } }
const shownRepos = computed(() => (repos.value || []).filter((r) => !repoQ.value || r.full_name.toLowerCase().includes(repoQ.value.toLowerCase())).slice(0, 8))
const linkRepo = (repo, branch) => ask('github_link_site', { ...base.value, repo, branch }, { title: `${t('repo.connect')}: ${repo} · ${branch}`, details: [t('repo.title')], after: () => loadServer(props.name) })
const deploy = () => ask('deploy_site', base.value, { title: `${t('repo.deploy')}: ${props.domain}`, after: refresh })
const headStatus = computed(() => {
  if (!head.value?.sha || !site.value.git) return null
  return head.value.sha.startsWith(site.value.git.sha) ? ['ok', t('repo.up_to_date')] : ['warn', t('repo.behind')]
})

// ---- DNS ----
const ip = computed(() => serverIp(props.name))
const dnsRows = computed(() => site.value.domains.filter((d) => d !== '*').map((d) => ({ host: d, st: dnsStatus(d, ip.value) })))
const fixable = computed(() => dnsRows.value.filter((r) => ['missing', 'elsewhere'].includes(r.st?.kind)))
function pointHere() {
  const hosts = fixable.value.map((r) => r.host)
  ask('cloudflare_upsert_record', { zone: zoneFor(hosts[0]).name, type: 'A', name: hosts[0], content: ip.value, proxied: true }, {
    title: t('dns.point_here'),
    details: hosts.map((h) => `A ${h} → ${ip.value} (proxied)`),
    after: async () => {
      for (const h of hosts.slice(1)) await tool('cloudflare_upsert_record', { zone: zoneFor(h).name, type: 'A', name: h, content: ip.value, proxied: true })
      await loadCloudflare()
    },
  })
}

// ---- mail (Resend) ----
const mailDomain = computed(() => site.value.domains.find((d) => d !== '*' && !d.startsWith('www.')))
const resendDomains = ref(null)
async function loadMail() { try { resendDomains.value = await tool('mail_domains') } catch { resendDomains.value = [] } }
if (state.integrations.resend?.connected) loadMail()
const rd = computed(() => (resendDomains.value || []).find((d) => d.name === mailDomain.value))
const mailReady = computed(() => env.value.mail_host === 'smtp.resend.com')
const full = (n, d) => (!n || n === '@' || n === d ? d : n.endsWith(`.${d}`) ? n : `${n}.${d}`)
const connectMail = () => ask('site_mail_resend', { server: props.name, domain: mailDomain.value }, {
  title: t('mail.plan_title', { domain: mailDomain.value }),
  details: [t('mail.step_domain', { domain: mailDomain.value }), t('mail.step_dns', { zone: zoneFor(mailDomain.value)?.name || '' }), t('mail.step_verify'), ...(laravel.value ? [t('mail.step_env', { path: site.value.app_path })] : [])],
  after: async () => { await loadMail(); await refresh() },
})

// ---- access ----
const person = ref('')
const key = ref('')
const shell = ref(false)
const grant = () => ask('site_access_grant', { ...base.value, person: person.value.trim(), public_key: key.value.trim(), shell: shell.value }, {
  title: `${shell.value ? 'SSH' : 'SFTP'} · ${person.value} · ${props.domain}`,
  details: [shell.value ? t('access.shell_hint') : t('access.sftp_hint'), t('access.scope', { owner: site.value.owner })],
  after: () => { person.value = ''; key.value = ''; return refresh() },
})
const revoke = (p) => ask('site_access_revoke', { ...base.value, person: p }, { title: `${t('web.remove')} ${p}`, risk: 'destructive', after: refresh })

// ---- processes ----
const procAction = (p, action) => ask('service_action', { server: props.name, kind: 'supervisor', name: p.name, action }, { title: `${t(`proc.${action}`)} ${p.name}`, after: refresh })
const stateColor = (s) => (['RUNNING', 'running'].includes(s) ? 'ok' : ['STARTING', 'BACKOFF'].includes(s) ? 'warn' : 'bad')
const users = computed(() => [...new Set(procs.value.map((p) => p.user).filter(Boolean))])
</script>

<template>
  <div v-if="!snap || !site" class="empty">{{ t('sites.none') }}</div>
  <div v-else class="stack">
    <router-link :to="`/servers/${encodeURIComponent(name)}/sites`" class="dim" style="text-decoration:none">← {{ t('sites.back') }}</router-link>
    <div class="row wrap">
      <div class="avatar" style="width:44px;height:44px;font-size:18px">{{ siteTitle(site, t).slice(0, 1).toUpperCase() }}</div>
      <div class="grow"><div style="font-size:20px;font-weight:700">{{ siteTitle(site, t) }}</div><div class="dim">{{ site.domains.join(' · ') }}</div></div>
      <a v-if="site.domains[0] !== '*'" class="btn" :href="`https://${site.domains[0]}`" target="_blank" rel="noopener"><Icon name="external" :size="14" />{{ t('sites.open') }}</a>
      <router-link v-if="site.app_path && can" class="btn" :to="`/servers/${encodeURIComponent(name)}/files/${encodeURIComponent(domain)}`"><Icon name="folder" :size="14" />{{ t('files.open') }}</router-link>
      <template v-if="laravel && can">
        <button class="btn" @click="maintenance(true)"><Icon name="construction" :size="14" />{{ t('sites.maintenance_on') }}</button>
        <button class="btn ghost" @click="maintenance(false)">{{ t('sites.maintenance_off') }}</button>
        <button v-if="!hasReverb" class="btn ghost" @click="reverb"><Icon name="zap" :size="14" />{{ t('sites.enable_reverb') }}</button>
      </template>
      <button v-if="can" class="btn ghost" :disabled="!managed" @click="toggle"><Icon name="power" :size="14" />{{ site.disabled ? t('sites.enable') : t('sites.disable') }}</button>
    </div>
    <div class="faint xs">{{ t('sites.disable_note') }}</div>

    <div class="card">
      <h3>{{ t('nav.sites') }}</h3>
      <div class="kv"><span>{{ t('sites.framework') }}</span><span>{{ frameworkLabel(site) || '—' }}</span></div>
      <div class="kv"><span>{{ t('sites.directory') }}</span><span class="mono">{{ site.app_path || '—' }}</span></div>
      <div class="kv"><span>{{ t('sites.served_by') }}</span><span>{{ site.served_by }}{{ site.php ? ' + PHP' : '' }}{{ site.upstreams.length ? ' → ' + site.upstreams.join(', ') : '' }}</span></div>
      <div class="kv"><span>{{ t('sites.tls') }}</span><span class="mono">{{ site.tls || t('sites.tls_none') }}</span></div>
      <template v-if="site.env">
        <div class="kv"><span>{{ t('sites.environment') }}</span><span>{{ env.app_env || '—' }}</span></div>
        <div class="kv"><span>{{ t('sites.database') }}</span><span>{{ env.db_connection ? `${env.db_connection}${env.db_database ? ` · ${env.db_database} @ ${env.db_host || ''}` : ''}` : '—' }}</span></div>
        <div class="kv"><span>{{ t('sites.drivers') }}</span><span>{{ t('sites.drivers_value', { cache: env.cache_store || '—', queue: env.queue_connection || '—', session: env.session_driver || '—' }) }}</span></div>
        <div class="kv"><span>{{ t('sites.mail') }}</span><span>{{ env.mail_mailer ? `${env.mail_mailer}${env.mail_host ? ` via ${env.mail_host}` : ''}` : '—' }}</span></div>
      </template>
    </div>

    <div class="card">
      <h3>{{ t('ns.services') }}</h3>
      <div v-if="!laravel" class="faint xs">Laravel only</div>
      <template v-else>
        <div class="svc-row"><span>{{ t('ns.database') }}</span><span :class="['pill', env.db_database ? 'ok' : '']">{{ env.db_database ? `${env.db_connection} · ${env.db_database}` : t('ns.none') }}</span><button v-if="!env.db_database && dbEngine && can" class="btn primary xs" @click="addDb"><Icon name="plus" :size="12" />{{ t('svc_site.add') }}</button></div>
        <div class="svc-row"><span>Redis</span><span :class="['pill', env.redis_username ? 'ok' : '']">{{ env.redis_username ? `ACL ${env.redis_username}` : t('ns.none') }}</span><button v-if="!env.redis_username && hasRedis && can" class="btn primary xs" @click="addRedis"><Icon name="plus" :size="12" />{{ t('svc_site.add') }}</button></div>
        <div class="svc-row"><span>{{ t('ns.queue') }}</span><span :class="['pill', workers ? 'ok' : '']">{{ workers ? t('svc_site.workers', { n: workers }) : t('ns.none') }}</span><button v-if="!workers && can" class="btn primary xs" @click="addQueue"><Icon name="plus" :size="12" />{{ t('svc_site.add') }}</button></div>
        <div class="svc-row"><span>Reverb</span><span :class="['pill', hasReverb ? 'ok' : '']">{{ hasReverb ? t('svc_site.on') : t('ns.none') }}</span><button v-if="!hasReverb && can" class="btn primary xs" @click="reverb"><Icon name="plus" :size="12" />{{ t('svc_site.add') }}</button></div>
        <div class="svc-row"><span>{{ t('ns.scheduler') }}</span><span :class="['pill', scheduled ? 'ok' : '']">{{ scheduled ? t('svc_site.on') : t('ns.none') }}</span><button v-if="!scheduled && can" class="btn primary xs" @click="addScheduler"><Icon name="plus" :size="12" />{{ t('svc_site.add') }}</button></div>
      </template>
    </div>

    <div v-if="site.domains[0] !== '*'" class="card">
      <h3>{{ t('sub.title') }}</h3>
      <div class="faint xs">{{ t('sub.hint') }}</div>
      <div v-if="!subs.length" class="dim">{{ t('sub.none') }}</div>
      <router-link v-for="s in subs" :key="s.domains[0]" :to="`/servers/${encodeURIComponent(name)}/sites/${encodeURIComponent(s.domains[0])}`" class="row sub-row"><Icon name="globe" :size="14" class="faint" /><span class="mono">{{ s.domains[0] }}</span><span class="faint xs">{{ s.app_path }}</span></router-link>
      <div v-if="can" class="row"><input v-model="sub" placeholder="api" style="width:160px"><span class="dim">.{{ domain }}</span><button class="btn primary xs" @click="addSub"><Icon name="plus" :size="12" />{{ t('sub.add') }}</button></div>
    </div>

    <div class="card">
      <h3>{{ t('repo.title') }}</h3>
      <div class="kv"><span>{{ t('repo.on_server') }}</span><span class="mono">{{ site.git ? `${site.git.branch || 'detached'} @ ${site.git.sha}${site.git.remote ? ' · ' + site.git.remote : ''}` : t('repo.not_git') }}</span></div>
      <template v-if="link">
        <div class="kv"><span>{{ t('repo.linked') }}</span><span class="row"><Brand name="github" :size="14" />{{ link.repo }} · {{ link.branch }} <span v-if="headStatus" :class="['pill', headStatus[0]]">{{ headStatus[1] }}</span></span></div>
        <div v-if="head?.sha" class="kv"><span>{{ t('repo.latest') }}</span><span class="mono">{{ head.sha.slice(0, 7) }} {{ head.message }} — {{ head.author }}</span></div>
        <div v-if="head?.error" class="error xs">{{ head.error }}</div>
        <div class="row"><button class="btn xs" @click="checkHead"><Icon name="refresh" :size="12" />{{ t('repo.check') }}</button><button v-if="can" class="btn primary xs" @click="deploy"><Icon name="rocket" :size="12" />{{ t('repo.deploy') }}</button></div>
      </template>
      <div v-else-if="!ghConnected" class="faint xs">{{ t('int.connect_first', { what: 'GitHub' }) }}</div>
      <template v-else-if="can && site.app_path">
        <div v-if="repos === null"><button class="btn xs" @click="loadRepos"><Brand name="github" :size="13" />{{ t('repo.search') }}</button></div>
        <template v-else>
          <input v-model="repoQ" :placeholder="t('repo.search')" style="max-width:320px">
          <div v-for="r in shownRepos" :key="r.full_name" class="row wrap repo-row">
            <span class="mono grow">{{ r.full_name }}</span>
            <button v-if="!branches[r.full_name]" class="btn xs" @click="loadBranches(r.full_name)">{{ t('repo.branch') }}</button>
            <template v-else><button v-for="b in branches[r.full_name]" :key="b" :class="['btn', 'xs', b === r.default_branch ? 'primary' : '']" :title="t('repo.connect')" @click="linkRepo(r.full_name, b)">{{ b }}</button></template>
          </div>
        </template>
      </template>
    </div>

    <div class="card">
      <h3>{{ t('dns.title') }}</h3>
      <div v-if="!state.integrations.cloudflare?.connected" class="faint xs">{{ t('int.connect_first', { what: 'Cloudflare' }) }}</div>
      <template v-else>
        <div v-for="r in dnsRows" :key="r.host" class="row">
          <span class="dot" :style="{ background: r.st?.kind === 'ok' ? 'var(--ok)' : r.st?.kind === 'no_zone' ? 'var(--faint)' : 'var(--warn)' }" />
          <span class="mono" style="width:260px">{{ r.host }}</span>
          <span class="dim">{{ r.st?.kind === 'ok' ? `${t('dns.ok')} · ${r.st.record.proxied ? 'proxied' : 'DNS only'}` : r.st?.kind === 'missing' ? t('dns.missing', { zone: r.st.zone }) : r.st?.kind === 'elsewhere' ? t('dns.elsewhere', { target: r.st.record.content }) : t('dns.no_zone') }}</span>
        </div>
        <div v-if="fixable.length && can && ip"><button class="btn primary xs" @click="pointHere"><Icon name="cloud" :size="12" />{{ t('dns.point_here') }}</button></div>
      </template>
    </div>

    <div class="card">
      <h3>{{ t('mail.title') }}</h3>
      <div v-if="!state.integrations.resend?.connected" class="faint xs">{{ t('int.connect_first', { what: 'Resend' }) }}</div>
      <template v-else-if="rd">
        <div class="row"><span class="dot" :style="{ background: rd.status === 'verified' ? 'var(--ok)' : 'var(--warn)' }" /><span class="mono">{{ rd.name }}</span><span :class="['pill', rd.status === 'verified' ? 'ok' : 'warn']">{{ rd.status.replace('_', ' ') }}</span><span class="faint xs">{{ rd.region }}</span></div>
        <div v-for="r in rd.records" :key="r.type + r.name + r.value" class="row xs"><span class="dot" :style="{ background: r.status === 'verified' ? 'var(--ok)' : 'var(--faint)' }" /><span class="dim" style="width:46px">{{ r.type }}</span><span class="mono" style="width:260px">{{ full(r.name, rd.name) }}</span><span class="faint grow" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ r.value }}</span></div>
        <div class="row">
          <span v-if="mailReady" class="xs" style="color:var(--ok)">{{ t('mail.site_ready') }}</span>
          <button v-else-if="laravel && can" class="btn primary xs" @click="connectMail"><Icon name="mail" :size="12" />{{ t('mail.connect') }}</button>
          <button v-if="rd.status !== 'verified' && can" class="btn ghost xs" @click="connectMail"><Icon name="refresh" :size="12" />{{ t('mail.verify') }}</button>
        </div>
      </template>
      <template v-else-if="mailDomain">
        <div class="dim">{{ laravel ? t('mail.lead_laravel') : t('mail.lead') }}</div>
        <div v-if="can"><button class="btn primary xs" :disabled="!zoneFor(mailDomain)" @click="connectMail"><Icon name="mail" :size="12" />{{ t('mail.connect') }}</button></div>
      </template>
    </div>

    <div class="card">
      <h3>{{ t('access.title') }}</h3>
      <div v-if="!site.owner" class="faint xs">{{ t('access.unknown') }}</div>
      <template v-else>
        <div class="row"><Icon name="user" :size="14" class="dim" /><span class="dim">{{ t('access.owner') }}</span><span class="mono">{{ site.owner }}</span><span v-if="site.owner === 'root'" class="pill warn">{{ t('access.root_owner') }}</span></div>
        <div v-if="site.owner === 'root'" class="faint xs">{{ t('access.root_hint') }}</div>
        <template v-else>
          <div v-if="!site.access.length" class="faint xs">{{ t('access.nobody') }}</div>
          <div v-for="a in site.access" :key="a.name" class="row wrap">
            <span class="dot" /><span class="b" style="width:110px">{{ a.name }}</span>
            <span :class="['pill', a.mode === 'shell' ? 'warn' : 'accent']">{{ a.mode.toUpperCase() }}</span>
            <span class="mono grow">{{ a.mode === 'shell' ? 'ssh' : 'sftp' }} {{ site.owner }}@{{ ip || name }}</span>
            <span class="faint xs">{{ a.key }}</span>
            <button v-if="can" class="btn ghost xs" @click="revoke(a.name)"><Icon name="x" :size="12" /></button>
          </div>
          <template v-if="can">
            <div class="b" style="margin-top:6px">{{ t('access.add') }}</div>
            <div class="row wrap"><input v-model="person" placeholder="anna" style="width:140px"><input v-model="key" class="grow mono" placeholder="ssh-ed25519 AAAA… anna@laptop"></div>
            <div class="row wrap">
              <label class="row" style="gap:6px"><input v-model="shell" type="checkbox" style="width:auto">{{ t('access.shell') }}</label>
              <span class="faint xs grow">{{ shell ? t('access.shell_hint') : t('access.sftp_hint') }}</span>
              <button class="btn primary xs" :disabled="!person.trim() || !key.trim()" @click="grant"><Icon name="key" :size="12" />{{ t('access.grant') }}</button>
            </div>
            <div class="faint xs">{{ t('access.scope', { owner: site.owner }) }}</div>
          </template>
        </template>
      </template>
    </div>

    <div v-if="procs.length" class="card">
      <h3>{{ t('sites.processes') }}</h3>
      <div v-for="p in procs" :key="p.name" class="row wrap proc-row">
        <span class="dot" :style="{ background: `var(--${stateColor(p.state)})` }" /><span :class="['pill', stateColor(p.state)]" style="width:84px;justify-content:center">{{ p.state }}</span>
        <span class="b grow">{{ p.name }}</span><span class="dim" style="width:70px">{{ p.role }}</span><span class="dim mono xs" style="width:170px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ p.user }}</span>
        <span class="faint" style="width:140px">{{ p.uptime ? t('proc.up', { time: p.uptime }) : '' }}</span>
        <template v-if="can">
          <template v-if="p.state === 'RUNNING'"><button class="btn ghost xs" :title="t('proc.restart')" @click="procAction(p, 'restart')"><Icon name="refresh" :size="13" /></button><button class="btn ghost xs" :title="t('proc.stop')" @click="procAction(p, 'stop')"><Icon name="square" :size="13" /></button></template>
          <button v-else class="btn ghost xs" :title="t('proc.start')" @click="procAction(p, 'start')"><Icon name="play" :size="13" /></button>
        </template>
      </div>
    </div>

    <div class="card">
      <h3>{{ t('sites.isolation') }}</h3>
      <div class="kv"><span>Linux</span><span>{{ users.join(', ') || site.owner || '—' }}</span></div>
    </div>
  </div>
</template>

<style>
.svc-row { display: grid; grid-template-columns: 160px 1fr auto; gap: 12px; align-items: center; min-height: 32px; }
.svc-row > span:first-child { color: var(--dim); }
.svc-row .pill { justify-self: start; }
.sub-row, .repo-row { padding: 4px 8px; border-radius: 6px; text-decoration: none; }
.sub-row:hover, .repo-row:hover { background: var(--hover); }
.proc-row { min-height: 30px; }
@media (max-width: 700px) { .svc-row { grid-template-columns: 1fr auto; } .svc-row > span:first-child { grid-column: 1 / -1; } }
</style>
