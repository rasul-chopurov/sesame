<script setup>
import { computed, onMounted, ref } from 'vue'
import { tool, session } from '../api.js'
import { t } from '../i18n.js'
import { ask } from '../runner.js'

const props = defineProps({ name: String })
const tab = ref('overview')
const summary = ref(null)
const sites = ref([])
const services = ref([])
const dbs = ref([])
const error = ref('')
const busy = ref(false)
const newDb = ref({ engine: 'postgres', name: '' })

// Engine names as the snapshot spells them → as the tools accept them.
const ENGINE = { my_sql: 'mysql', maria_db: 'mariadb', postgres: 'postgres', mongo_db: 'mongodb' }
const LABEL = { my_sql: 'MySQL', maria_db: 'MariaDB', postgres: 'PostgreSQL', redis: 'Redis', mongo_db: 'MongoDB' }
const canChange = computed(() => session.user && session.user.role !== 'viewer')
const server = () => ({ server: props.name })

async function load() {
  error.value = ''
  try {
    const [s, st, sv, d] = await Promise.all([tool('get_server', server()), tool('list_sites', server()), tool('list_services', server()), tool('list_databases', server())])
    summary.value = s
    sites.value = st
    services.value = sv
    dbs.value = d
  } catch (e) {
    error.value = e.message
  }
}

async function rediscover() {
  busy.value = true
  try { await tool('discover_server', server()); await load() } catch (e) { error.value = e.message } finally { busy.value = false }
}

const levelClass = (l) => ({ critical: 'bad', warning: 'warn', info: '' })[l] ?? ''
const managed = (s) => (s.app_path || '').startsWith('/home/s_')
const laravel = (s) => s.framework === 'laravel'
const domain = (s) => s.domains?.[0] || ''

function sizeOf(engine, db) {
  const hit = (engine.sizes || []).find(([n]) => n === db)
  if (!hit) return ''
  const mb = hit[1] / 1024 / 1024
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb.toFixed(1)} MB`
}

onMounted(load)
</script>

<template>
  <div class="page-title">
    <div>
      <h1>{{ name }}</h1>
      <div v-if="summary" class="dim">{{ summary.os?.distro }} · {{ t('discovered') }} {{ summary.taken_at?.replace('T', ' ').slice(0, 16) }}</div>
    </div>
    <button class="btn primary" :disabled="busy" @click="rediscover">↻ {{ busy ? t('running') : t('rediscover') }}</button>
  </div>
  <p v-if="error" class="error">{{ error }}</p>

  <div class="tabs">
    <button v-for="k in ['overview', 'sites', 'services', 'databases']" :key="k" :class="{ on: tab === k }" @click="tab = k">{{ t(k) }}</button>
  </div>

  <div v-if="summary && tab === 'overview'" class="stack">
    <div class="card">
      <h3>{{ t('attention') }}</h3>
      <p v-if="!summary.insights.length" class="dim">{{ t('all_good') }}</p>
      <div v-for="(i, n) in summary.insights" :key="n" class="row" style="margin:6px 0;align-items:flex-start">
        <span :class="['pill', levelClass(i.level)]">{{ i.level }}</span><span>{{ i.message }}</span>
      </div>
    </div>
    <div class="grid">
      <div class="card"><h3>{{ t('sites') }}</h3><div v-for="s in summary.sites" :key="s.domains[0]" class="mono">{{ s.domains[0] }}</div></div>
      <div class="card"><h3>{{ t('databases') }}</h3><div v-for="d in summary.databases" :key="d.kind">{{ LABEL[d.kind] || d.kind }} <span class="faint">{{ d.version }}</span></div></div>
      <div class="card"><h3>supervisor</h3><div v-for="p in summary.processes" :key="p.name" class="row"><span class="dot" :style="{ background: p.state === 'RUNNING' ? 'var(--ok)' : 'var(--bad)' }"></span><span class="mono">{{ p.name }}</span></div></div>
    </div>
  </div>

  <div v-if="tab === 'sites'" class="grid">
    <div v-for="s in sites" :key="domain(s)" class="card site">
      <div class="row" style="justify-content:space-between">
        <b class="mono">{{ domain(s) }}</b>
        <span v-if="s.disabled" class="pill">{{ t('disabled') }}</span>
        <span v-else-if="s.framework && s.framework !== 'unknown'" class="pill ok">{{ s.framework }} {{ s.framework_version || '' }}</span>
      </div>
      <div class="mono faint">{{ s.app_path || '—' }}</div>
      <div v-if="s.owner" class="faint">{{ t('owner') }}: <span class="mono">{{ s.owner }}</span></div>
      <div v-if="s.git" class="faint">git: <span class="mono">{{ s.git.branch }} @ {{ s.git.sha }}</span></div>
      <div v-if="canChange" class="row" style="flex-wrap:wrap;margin-top:6px">
        <template v-if="laravel(s)">
          <button class="btn sm" @click="ask('site_maintenance', { ...server(), domain: domain(s), on: true }, { title: `${t('maintenance_on')}: ${domain(s)}`, after: load })">{{ t('maintenance_on') }}</button>
          <button class="btn sm" @click="ask('site_maintenance', { ...server(), domain: domain(s), on: false }, { title: `${t('maintenance_off')}: ${domain(s)}`, after: load })">{{ t('maintenance_off') }}</button>
        </template>
        <button v-if="s.git" class="btn sm" @click="ask('deploy_site', { ...server(), domain: domain(s) }, { title: `${t('deploy')}: ${domain(s)}`, after: load })">{{ t('deploy') }}</button>
        <button v-if="managed(s)" class="btn sm" @click="ask('site_enabled', { ...server(), domain: domain(s), enabled: s.disabled }, { title: `${s.disabled ? t('site_on') : t('site_off')}: ${domain(s)}`, risk: 'destructive', word: s.disabled ? '' : domain(s), after: load })">{{ s.disabled ? t('site_on') : t('site_off') }}</button>
      </div>
    </div>
  </div>

  <div v-if="tab === 'services'" class="grid">
    <div v-for="s in services" :key="s.service" class="card">
      <div class="row" style="justify-content:space-between"><b>{{ s.name }}</b><span :class="['pill', s.installed ? 'ok' : '']">{{ s.installed ? `${t('installed')} ${s.version || ''}` : t('not_installed') }}</span></div>
      <p v-if="s.conflict" class="faint">⚠ {{ s.conflict }}</p>
      <div v-if="canChange" class="row" style="margin-top:8px">
        <button v-if="!s.installed" class="btn sm primary" :disabled="!!s.conflict" @click="ask('install_service', { ...server(), service: s.service }, { title: `${t('install')} ${s.name}`, details: [t('secrets_hidden')], after: rediscover })">{{ t('install') }}</button>
        <button v-else class="btn sm" @click="ask('uninstall_service', { ...server(), service: s.service }, { title: `${t('uninstall')} ${s.name}`, risk: 'destructive', word: s.service, after: rediscover })">{{ t('uninstall') }}</button>
      </div>
    </div>
  </div>

  <div v-if="tab === 'databases'" class="stack">
    <form v-if="canChange" class="card row" style="flex-wrap:wrap" @submit.prevent="ask('create_database', { ...server(), engine: newDb.engine, name: newDb.name }, { title: `${t('create_db')} ${newDb.name}`, details: [t('secrets_hidden')], after: rediscover })">
      <select v-model="newDb.engine" style="width:auto"><option v-for="(v, k) in ENGINE" :key="k" :value="v">{{ LABEL[k] }}</option></select>
      <input v-model="newDb.name" pattern="[A-Za-z0-9_]+" placeholder="shop" required style="width:200px">
      <button class="btn primary">{{ t('create_db') }}</button>
    </form>
    <div v-for="e in dbs" :key="e.kind" class="card">
      <h3>{{ LABEL[e.kind] || e.kind }} <span class="faint">{{ e.version }}</span></h3>
      <table>
        <tbody>
          <tr v-for="d in e.databases" :key="d">
            <td class="mono">{{ d }}</td>
            <td class="faint">{{ sizeOf(e, d) }}</td>
            <td style="text-align:right"><button v-if="canChange && ENGINE[e.kind]" class="btn sm" @click="ask('drop_database', { ...server(), engine: ENGINE[e.kind], name: d }, { title: `${t('drop')} ${d}`, risk: 'destructive', word: d, after: rediscover })">{{ t('drop') }}</button></td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.site { display: grid; gap: 4px; }
</style>
