<script setup>
import { computed, ref } from 'vue'
import { session } from '../../api.js'
import { t } from '../../i18n.js'
import { ask } from '../../runner.js'
import { rediscover } from '../../store.js'
import { DB, bytes, siteTitle, canChange } from '../../sections.js'
import Icon from '../../components/Icon.vue'
import Brand from '../../components/Brand.vue'

const props = defineProps({ name: String, snap: Object })
const open = ref(null)
const q = ref('')
const filter = ref('all')
const dbName = ref('')
const dbUser = ref('')
const engine = computed(() => props.snap.databases.find((d) => d.kind === open.value))
const can = computed(() => canChange(session.user))

const usedBy = (e) => Object.fromEntries(props.snap.sites.filter((s) => s.env?.db_database && e.databases.includes(s.env.db_database)).map((s) => [s.env.db_database, siteTitle(s, t)]))
const size = (e, d) => { const s = (e.sizes || []).find(([n]) => n === d); return s ? bytes(s[1]) : '—' }
const rows = computed(() => {
  if (!engine.value) return []
  const used = usedBy(engine.value), s = q.value.trim().toLowerCase()
  return engine.value.databases
    .map((d) => ({ name: d, size: size(engine.value, d), site: used[d] }))
    .filter((r) => !s || r.name.toLowerCase().includes(s) || (r.site || '').toLowerCase().includes(s))
    .filter((r) => filter.value === 'all' || (filter.value === 'used' ? r.site : !r.site))
})
const refresh = () => rediscover(props.name)
function create() {
  const n = dbName.value.trim()
  if (!/^[A-Za-z0-9_]+$/.test(n)) return
  ask('create_database', { server: props.name, engine: DB[open.value].tool, name: n, user: dbUser.value.trim() || undefined }, {
    title: `${t('db.create')}: ${n}`, after: () => { dbName.value = ''; dbUser.value = ''; return refresh() },
  })
}
const drop = (r) => ask('drop_database', { server: props.name, engine: DB[open.value].tool, name: r.name }, { title: `${t('db.drop')}: ${r.name}`, risk: 'destructive', word: r.name, after: refresh })
</script>

<template>
  <div v-if="!snap.databases.length" class="empty">{{ t('db.none') }}</div>
  <div v-else-if="!engine" class="grid2">
    <button v-for="e in snap.databases" :key="e.kind" class="card engine" @click="open = e.kind">
      <div class="row"><div class="logo"><Brand :name="DB[e.kind]?.logo" :size="22" /></div><div class="grow" style="text-align:left"><div class="b">{{ DB[e.kind]?.name }}</div><div class="dim xs">{{ e.version }}</div></div><span class="pill">{{ t('db.count', { n: e.databases.length }) }}</span></div>
      <div v-if="e.listen" class="kv xs"><span>{{ t('db.listens') }}</span><span class="mono">{{ e.listen }}</span></div>
    </button>
  </div>
  <div v-else class="stack">
    <a class="dim link" style="color:var(--dim)" @click="open = null">← {{ t('db.all_engines') }}</a>
    <div class="row"><div class="logo"><Brand :name="DB[engine.kind]?.logo" :size="22" /></div><div><div style="font-size:18px;font-weight:700">{{ DB[engine.kind]?.name }}</div><div class="dim">{{ engine.version }}<template v-if="engine.listen"> · {{ engine.listen }}</template></div></div></div>
    <div v-if="DB[engine.kind]?.tool && can" class="card">
      <h3>{{ t('db.new') }}</h3>
      <div class="row wrap">
        <input v-model="dbName" :placeholder="t('db.name_placeholder')" style="width:220px" @keyup.enter="create">
        <input v-model="dbUser" :placeholder="t('db.user_placeholder')" style="width:220px" @keyup.enter="create">
        <button class="btn primary" :disabled="!/^[A-Za-z0-9_]+$/.test(dbName.trim())" @click="create"><Icon name="plus" :size="14" />{{ t('db.create') }}</button>
      </div>
    </div>
    <div v-if="engine.kind === 'redis'" class="faint xs">{{ t('db.redis_note') }}</div>
    <div class="card">
      <h3>{{ t('db.databases') }}</h3>
      <div class="row wrap">
        <div class="search grow" style="max-width:320px"><Icon name="search" :size="14" class="faint" /><input v-model="q" :placeholder="t('db.search')"></div>
        <button v-for="[k, l] in [['all', t('sites.all')], ['used', t('db.used')], ['unused', t('db.unused')]]" :key="k" class="chip" :class="{ on: filter === k }" @click="filter = k">{{ l }}</button>
      </div>
      <div v-if="!engine.databases.length" class="dim">{{ t('db.needs_creds') }}</div>
      <div v-else-if="!rows.length" class="dim">{{ t('db.no_match') }}</div>
      <table v-else>
        <thead><tr><th>{{ t('db.name') }}</th><th>{{ t('db.size') }}</th><th>{{ t('db.used_by') }}</th><th /></tr></thead>
        <tbody>
          <tr v-for="r in rows" :key="r.name">
            <td class="mono">{{ r.name }}</td><td class="dim">{{ r.size }}</td><td>{{ r.site || '—' }}</td>
            <td style="text-align:right"><button v-if="DB[engine.kind]?.tool && can" class="btn ghost xs" @click="drop(r)"><Icon name="trash" :size="12" />{{ t('db.drop') }}</button></td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style>
.engine { cursor: pointer; text-align: left; font: inherit; color: inherit; }
.engine:hover { border-color: color-mix(in srgb, var(--accent) 50%, transparent); background: var(--hover); }
</style>
