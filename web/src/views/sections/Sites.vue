<script setup>
import { computed, ref } from 'vue'
import { session } from '../../api.js'
import { t } from '../../i18n.js'
import { state, dnsStatus, serverIp, zoneFor, loadCloudflare } from '../../store.js'
import { ask } from '../../runner.js'
import { frameworkLabel, siteTitle, canChange } from '../../sections.js'
import Icon from '../../components/Icon.vue'
import Brand from '../../components/Brand.vue'

const props = defineProps({ name: String, snap: Object })
const q = ref('')
const kind = ref(null)
const kinds = computed(() => [...new Set(props.snap.sites.map((s) => s.framework).filter((f) => f !== 'unknown'))])
const visible = computed(() => {
  const s = q.value.trim().toLowerCase()
  return props.snap.sites
    .filter((x) => !kind.value || x.framework === kind.value)
    .filter((x) => !s || x.domains.some((d) => d.toLowerCase().includes(s)) || (x.app_path || '').toLowerCase().includes(s) || frameworkLabel(x).toLowerCase().includes(s))
})
const procs = (site) => props.snap.supervisor.filter((p) => site.processes.includes(p.name))
const healthy = (site) => procs(site).every((p) => p.state === 'RUNNING')
const href = (site) => `/servers/${encodeURIComponent(props.name)}/sites/${encodeURIComponent(site.domains[0])}`
const fwLogo = (f) => ({ laravel: 'laravel', word_press: 'wordpress', node: 'nodedotjs' })[f]

// Hosts not pointed at this server yet (the desktop's DNS banner).
const ip = computed(() => serverIp(props.name))
const unpointed = computed(() => {
  if (!ip.value) return []
  const hosts = props.snap.sites.flatMap((s) => s.domains).filter((d) => d !== '*' && d.includes('.') && !/^\d+\./.test(d))
  return [...new Set(hosts)].filter((h) => ['missing', 'elsewhere'].includes(dnsStatus(h, ip.value)?.kind))
})
function pointAll() {
  const byZone = {}
  for (const h of unpointed.value) {
    const z = zoneFor(h)
    if (z) (byZone[z.name] ||= []).push(h)
  }
  const first = Object.entries(byZone)
  const runAll = async () => {
    const { tool } = await import('../../api.js')
    for (const [zone, hosts] of first) for (const h of hosts) await tool('cloudflare_upsert_record', { zone, type: 'A', name: h, content: ip.value, proxied: true })
    await loadCloudflare()
  }
  ask('cloudflare_upsert_record', { zone: first[0][0], type: 'A', name: first[0][1][0], content: ip.value, proxied: true }, {
    title: t('dns.sync', { n: unpointed.value.length }),
    details: unpointed.value.map((h) => `A ${h} → ${ip.value} (proxied)`),
    after: runAll,
  })
}
</script>

<template>
  <div class="stack">
    <div class="row wrap">
      <div class="search grow" style="max-width:380px"><Icon name="search" :size="14" class="faint" /><input v-model="q" :placeholder="t('sites.search')"></div>
      <button class="chip" :class="{ on: !kind }" @click="kind = null">{{ t('sites.all') }}</button>
      <button v-for="k in kinds" :key="k" class="chip" :class="{ on: kind === k }" @click="kind = k">{{ frameworkLabel({ framework: k }) }}</button>
      <div class="grow" />
      <router-link v-if="canChange(session.user)" :to="`/servers/${encodeURIComponent(name)}/new-site`" class="btn primary"><Icon name="plus" :size="14" />{{ t('ns.button') }}</router-link>
    </div>

    <div v-if="unpointed.length && canChange(session.user)" class="card banner">
      <div class="row wrap">
        <img src="/brands/cloudflare.svg" width="16" height="16" alt="">
        <span class="grow">{{ unpointed.join(', ') }}</span>
        <button class="btn primary xs" @click="pointAll">{{ t('dns.sync', { n: unpointed.length }) }}</button>
      </div>
    </div>

    <div v-if="!snap.sites.length" class="empty">{{ t('sites.none') }}</div>
    <div v-else-if="!visible.length" class="empty">{{ t('sites.no_match', { q }) }}</div>
    <div v-else class="cards">
      <router-link v-for="site in visible" :key="site.domains.join()" :to="href(site)" class="card site-card">
        <div class="row">
          <div class="avatar">{{ siteTitle(site, t).slice(0, 1).toUpperCase() }}</div>
          <div class="grow">
            <div class="b" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ siteTitle(site, t) }}</div>
            <div class="faint xs" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ site.domains.slice(1).join(' · ') || site.app_path }}</div>
          </div>
          <span class="dot" :style="{ background: site.disabled ? 'var(--faint)' : healthy(site) ? 'var(--ok)' : 'var(--bad)' }" />
        </div>
        <div class="row wrap" style="gap:6px">
          <span v-if="site.disabled" class="pill">{{ t('sites.disabled') }}</span>
          <span v-if="frameworkLabel(site)" class="pill"><Brand :name="fwLogo(site.framework)" :size="12" />{{ frameworkLabel(site) }}</span>
          <span v-if="procs(site).some((p) => p.role === 'octane')" class="pill accent">Octane</span>
          <span v-else-if="site.php" class="pill info">PHP</span>
          <span v-if="site.tls" class="pill ok"><Icon name="lock" :size="11" />TLS</span>
        </div>
        <div class="row faint xs" style="gap:12px">
          <span v-if="site.env?.db_database"><Icon name="database" :size="12" /> {{ site.env.db_database }}</span>
          <span v-if="procs(site).length"><Icon name="activity" :size="12" /> {{ procs(site).length }}</span>
          <span v-if="site.owner" class="mono">{{ site.owner }}</span>
        </div>
      </router-link>
    </div>
  </div>
</template>

<style>
.search { display: flex; align-items: center; gap: 6px; padding: 0 10px; border: 1px solid var(--line); border-radius: 7px; background: var(--panel-solid); }
.search input { border: 0; background: none; padding: 6px 0; outline: none; }
.site-card { text-decoration: none; cursor: pointer; }
.site-card:hover { border-color: color-mix(in srgb, var(--accent) 50%, transparent); background: var(--hover); }
.banner { border-color: color-mix(in srgb, var(--accent) 40%, transparent); background: var(--accent-soft); }
</style>
