<script setup>
import { computed, ref } from 'vue'
import { session } from '../api.js'
import { t } from '../i18n.js'
import { ask } from '../runner.js'
import { state, loadCloudflare } from '../store.js'
import { canChange } from '../sections.js'
import Icon from '../components/Icon.vue'

const TYPES = ['A', 'AAAA', 'CNAME', 'TXT', 'MX', 'NS', 'CAA']
const proxiable = (k) => ['A', 'AAAA', 'CNAME'].includes(k)
const can = computed(() => canChange(session.user))
const selected = ref(null)
const zq = ref('')
const rq = ref('')
const type = ref(null)
const newZone = ref('')
const add = ref({ type: 'A', name: '', content: '', ttl: 1, proxied: true, priority: 10 })

const zones = computed(() => state.zones.filter((z) => !zq.value || z.name.includes(zq.value.trim().toLowerCase())))
const zone = computed(() => state.zones.find((z) => z.name === selected.value))
const records = computed(() => (state.records[selected.value] || [])
  .filter((r) => !type.value || r.type === type.value)
  .filter((r) => !rq.value || `${r.name} ${r.content}`.toLowerCase().includes(rq.value.toLowerCase())))
const types = computed(() => [...new Set((state.records[selected.value] || []).map((r) => r.type))])
const short = (name) => (name === selected.value ? '@' : name.replace(`.${selected.value}`, ''))
const full = (n) => { n = n.trim().toLowerCase(); return !n || n === '@' ? selected.value : n.endsWith(selected.value) ? n : `${n}.${selected.value}` }

function addZone() {
  const n = newZone.value.trim().toLowerCase()
  if (!n.includes('.')) return
  ask('cloudflare_create_zone', { name: n }, { title: `${t('cf.add_zone')}: ${n}`, details: [t('cf.add_zone_hint')], after: () => { newZone.value = ''; return loadCloudflare() } })
}
function addRecord() {
  const a = add.value
  if (!a.content.trim()) return
  const args = { zone: selected.value, type: a.type, name: full(a.name), content: a.content.trim(), ttl: Number(a.ttl) || 1, proxied: proxiable(a.type) && a.proxied }
  if (a.type === 'MX') args.priority = Number(a.priority) || 10
  ask('cloudflare_upsert_record', args, { title: `${a.type} ${args.name} → ${args.content}`, after: () => { add.value = { ...add.value, name: '', content: '' }; return loadCloudflare() } })
}
const del = (r) => ask('cloudflare_delete_record', { zone: selected.value, name: r.name, type: r.type }, { title: `${t('cf.delete')}: ${r.type} ${r.name}`, risk: 'destructive', after: loadCloudflare })
const toggleProxy = (r) => ask('cloudflare_upsert_record', { zone: selected.value, type: r.type, name: r.name, content: r.content, ttl: r.ttl, proxied: !r.proxied }, { title: `${r.name}: ${r.proxied ? t('cf.dns_only') : t('cf.proxied')}`, after: loadCloudflare })
const purge = () => ask('cloudflare_purge', { zone: selected.value }, { title: `${t('cf.purge')}: ${selected.value}` })
</script>

<template>
  <div class="stack">
    <div class="head"><h1>Cloudflare</h1></div>
    <div v-if="!state.integrations.cloudflare?.connected" class="card dim">{{ t('int.connect_first', { what: 'Cloudflare' }) }} <router-link to="/settings" class="link">{{ t('nav.settings') }}</router-link></div>
    <template v-else-if="!zone">
      <div class="row wrap">
        <div class="search grow" style="max-width:320px"><Icon name="search" :size="14" class="faint" /><input v-model="zq" :placeholder="t('cf.search_domains')"></div>
        <div class="grow" />
        <template v-if="can"><input v-model="newZone" placeholder="example.com" style="width:200px" @keyup.enter="addZone"><button class="btn primary" @click="addZone"><Icon name="plus" :size="14" />{{ t('cf.add_zone') }}</button></template>
      </div>
      <div v-if="!state.zones.length" class="empty">{{ t('cf.no_domains') }}</div>
      <div class="cards">
        <button v-for="z in zones" :key="z.id" class="card engine" @click="selected = z.name">
          <div class="row"><img src="/brands/cloudflare.svg" width="18" height="18" alt=""><b class="grow">{{ z.name }}</b><span :class="['pill', z.status === 'active' ? 'ok' : 'warn']">{{ z.status }}</span></div>
          <div class="dim xs">{{ t('cf.records_count', { n: (state.records[z.name] || []).length }) }}</div>
          <div v-if="z.status !== 'active'" class="faint xs mono">{{ t('cf.nameservers') }}: {{ z.name_servers.join(', ') }}</div>
        </button>
      </div>
    </template>
    <template v-else>
      <a class="link" style="color:var(--dim)" @click="selected = null">← {{ t('cf.all_domains') }}</a>
      <div class="row wrap"><b style="font-size:18px">{{ zone.name }}</b><span :class="['pill', zone.status === 'active' ? 'ok' : 'warn']">{{ zone.status }}</span><span class="faint xs mono">{{ t('cf.nameservers') }}: {{ zone.name_servers.join(', ') }}</span><div class="grow" /><button v-if="can" class="btn" @click="purge">{{ t('cf.purge') }}</button></div>
      <div v-if="can" class="card">
        <h3>{{ t('cf.add_record') }}</h3>
        <div class="row wrap">
          <select v-model="add.type" style="width:100px"><option v-for="k in TYPES" :key="k">{{ k }}</option></select>
          <input v-model="add.name" :placeholder="t('cf.name') + ' (@, www)'" style="width:180px">
          <input v-model="add.content" :placeholder="t('cf.content')" class="grow" style="min-width:200px">
          <input v-if="add.type === 'MX'" v-model="add.priority" type="number" :placeholder="t('cf.priority')" style="width:90px">
          <select v-model="add.ttl" style="width:110px"><option :value="1">{{ t('cf.auto') }}</option><option :value="300">5 min</option><option :value="3600">1 h</option><option :value="86400">1 day</option></select>
          <label v-if="proxiable(add.type)" class="row xs" style="gap:4px"><input v-model="add.proxied" type="checkbox" style="width:auto">{{ t('cf.proxied') }}</label>
          <button class="btn primary" @click="addRecord"><Icon name="plus" :size="14" />{{ t('cf.add_record') }}</button>
        </div>
        <div class="faint xs">{{ t('cf.add_hint') }}</div>
      </div>
      <div class="card">
        <div class="row wrap">
          <div class="search" style="width:260px"><Icon name="search" :size="14" class="faint" /><input v-model="rq" :placeholder="t('cf.search_records')"></div>
          <button class="chip" :class="{ on: !type }" @click="type = null">{{ t('sites.all') }}</button>
          <button v-for="k in types" :key="k" class="chip" :class="{ on: type === k }" @click="type = k">{{ k }}</button>
        </div>
        <div v-if="!records.length" class="dim">{{ t('cf.no_records') }}</div>
        <table v-else>
          <thead><tr><th>{{ t('cf.type') }}</th><th>{{ t('cf.name') }}</th><th>{{ t('cf.content') }}</th><th>{{ t('cf.ttl') }}</th><th /><th /></tr></thead>
          <tbody>
            <tr v-for="r in records" :key="r.id">
              <td><span class="pill">{{ r.type }}</span></td>
              <td class="mono">{{ short(r.name) }}</td>
              <td class="mono" style="overflow-wrap:anywhere;max-width:420px">{{ r.priority != null ? `${r.priority} ` : '' }}{{ r.content }}</td>
              <td class="dim">{{ r.ttl === 1 ? t('cf.auto') : r.ttl }}</td>
              <td><button v-if="proxiable(r.type)" :class="['chip', r.proxied ? 'on' : '']" :disabled="!can" @click="toggleProxy(r)">{{ r.proxied ? t('cf.proxied') : t('cf.dns_only') }}</button></td>
              <td style="text-align:right"><button v-if="can" class="btn ghost xs" :title="t('cf.delete')" @click="del(r)"><Icon name="trash" :size="12" /></button></td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
</template>
