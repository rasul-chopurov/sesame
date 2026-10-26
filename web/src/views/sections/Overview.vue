<script setup>
import { computed, onMounted, ref } from 'vue'
import { tool, session } from '../../api.js'
import { t, insight } from '../../i18n.js'
import { ask } from '../../runner.js'
import { rediscover } from '../../store.js'
import { DB, bytes, duration, canChange } from '../../sections.js'
import Icon from '../../components/Icon.vue'
import Brand from '../../components/Brand.vue'

const props = defineProps({ name: String, snap: Object })
const insights = ref([])
onMounted(async () => { try { insights.value = await tool('get_insights', { server: props.name }) } catch { /* ignore */ } })

const os = computed(() => props.snap.os)
const memUsed = computed(() => os.value.mem_total_bytes - os.value.mem_available_bytes)
const pct = (u, tot) => (tot ? Math.round((u / tot) * 100) : 0)
const meterColor = (p) => (p >= 90 ? 'var(--bad)' : p >= 75 ? 'var(--warn)' : 'var(--accent)')
const level = { critical: ['bad', 'alert', 'level.critical'], warning: ['warn', 'alert', 'level.warning'], info: ['info', 'info', 'level.info'] }
const webLogo = (k) => ({ caddy: 'caddy', franken_php: 'caddy', nginx: 'nginx', apache: 'apache' })[k] || 'caddy'

// What a fresh server is missing for Laravel (the desktop's stack::missing).
const missing = computed(() => {
  const s = props.snap, m = []
  if (!s.web_servers.length) m.push('web server')
  if (!s.php.length) m.push('PHP')
  if (!s.services.some((u) => u.unit.startsWith('supervisor'))) m.push('supervisor')
  if (s.firewall?.ufw !== 'active') m.push('firewall')
  return m
})

function prepare() {
  ask('prepare_server', { server: props.name }, { title: t('prepare.title'), details: [t('prepare.lead', { what: missing.value.join(', ') })], after: () => rediscover(props.name) })
}
function fixFpm(i) {
  const v = (i.params.find(([k]) => k === 'version')?.[1] || i.params[0]?.[1] || '').split('.').slice(0, 2).join('.')
  ask('service_action', { server: props.name, kind: 'systemd', name: `php${v}-fpm`, action: 'disable' }, { title: `${t('overview.fix_fpm')}: php${v}-fpm`, after: () => rediscover(props.name) })
}
const sitesCount = computed(() => props.snap.sites.filter((x) => x.domains.join() !== '*').length)
const dbCount = computed(() => props.snap.databases.reduce((n, d) => n + d.databases.length, 0))
</script>

<template>
  <div class="stack">
    <div v-if="missing.length && canChange(session.user)" class="card">
      <h3>{{ t('prepare.title') }}</h3>
      <div class="dim">{{ t('prepare.lead', { what: missing.join(', ') }) }}</div>
      <div><button class="btn primary" @click="prepare"><Icon name="rocket" :size="14" />{{ t('prepare.button') }}</button></div>
    </div>

    <div class="card">
      <h3>{{ t('overview.attention') }}</h3>
      <div v-if="!insights.length && !snap.warnings.length" class="row"><Icon name="check" :size="15" style="color:var(--ok)" />{{ t('overview.all_good') }}</div>
      <div v-for="(i, n) in insights" :key="n" class="row" style="min-height:26px">
        <Icon :name="level[i.level][1]" :size="15" :style="{ color: `var(--${level[i.level][0]})` }" />
        <span :class="['pill', level[i.level][0]]" style="min-width:76px;justify-content:center">{{ t(level[i.level][2]) }}</span>
        <span class="grow">{{ insight(i) }}</span>
        <button v-if="i.code === 'fpm_unused' && canChange(session.user)" class="btn ghost xs" @click="fixFpm(i)"><Icon name="power" :size="13" />{{ t('overview.fix_fpm') }}</button>
      </div>
      <div v-for="w in snap.warnings" :key="w" class="row"><span class="pill">{{ t('level.partial') }}</span>{{ w }}</div>
    </div>

    <div class="grid2">
      <div class="card">
        <h3>{{ t('overview.machine') }}</h3>
        <div class="kv"><span>{{ t('overview.hostname') }}</span><span>{{ os.hostname }}</span></div>
        <div class="kv"><span>{{ t('overview.system') }}</span><span>{{ os.distro }}</span></div>
        <div class="kv"><span>{{ t('overview.kernel') }}</span><span>{{ os.kernel }} ({{ os.arch }})</span></div>
        <div class="kv"><span>{{ t('overview.cpus') }}</span><span>{{ os.cpus }}</span></div>
        <div class="kv"><span>{{ t('overview.uptime') }}</span><span>{{ duration(os.uptime_seconds) }}</span></div>
        <div class="kv"><span>{{ t('overview.access') }}</span><span>{{ os.privileged ? t('overview.access_root') : t('overview.access_limited') }}</span></div>
      </div>
      <div class="card" style="gap:12px">
        <h3>{{ t('overview.resources') }}</h3>
        <div v-for="[label, used, total] in [[t('overview.memory'), memUsed, os.mem_total_bytes], [t('overview.disk'), os.disk_used_bytes, os.disk_total_bytes]]" :key="label" class="stack" style="gap:4px">
          <div class="row between"><span class="dim">{{ label }}</span><span>{{ t('overview.of', { used: bytes(used), total: bytes(total), pct: pct(used, total) }) }}</span></div>
          <div class="meter"><div :style="{ width: pct(used, total) + '%', background: meterColor(pct(used, total)) }" /></div>
        </div>
        <div class="kv"><span>{{ t('overview.load') }}</span><span>{{ os.load.map((x) => x.toFixed(2)).join('  ') }}</span></div>
      </div>
      <div class="card">
        <h3>{{ t('overview.stack') }}</h3>
        <div v-for="w in snap.web_servers" :key="w.kind" class="row"><Brand :name="webLogo(w.kind)" :size="15" /><span class="dim" style="width:110px">{{ t('overview.web') }}</span><span class="grow">{{ w.version || w.kind }}</span></div>
        <div v-for="p in snap.php" :key="p.version" class="row"><Brand name="php" :size="15" /><span class="dim" style="width:110px">PHP</span><span>{{ p.version }}</span></div>
        <div v-for="d in snap.databases" :key="d.kind" class="row"><Brand :name="DB[d.kind]?.logo" :size="15" /><span class="dim" style="width:110px">{{ DB[d.kind]?.name }}</span><span class="grow">{{ d.version }}</span></div>
      </div>
      <div class="card">
        <h3>{{ t('overview.found') }}</h3>
        <div class="kv"><span>{{ t('nav.sites') }}</span><span>{{ sitesCount }}</span></div>
        <div class="kv"><span>{{ t('nav.processes') }}</span><span>{{ snap.supervisor.length }}</span></div>
        <div class="kv"><span>{{ t('nav.databases') }}</span><span>{{ dbCount }}</span></div>
        <div class="kv"><span>{{ t('overview.cron_jobs') }}</span><span>{{ snap.cron.length }}</span></div>
        <div class="kv"><span>{{ t('overview.certificates') }}</span><span>{{ snap.certificates.length }}</span></div>
      </div>
    </div>
  </div>
</template>
