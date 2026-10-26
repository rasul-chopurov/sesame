<script setup>
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { t } from '../../i18n.js'
import { ask } from '../../runner.js'
import { rediscover } from '../../store.js'
import Icon from '../../components/Icon.vue'
import Brand from '../../components/Brand.vue'

const props = defineProps({ name: String, snap: Object })
const route = useRoute()
const router = useRouter()
const f = ref({ domain: route.query.domain || '', kind: 'laravel', code: 'skeleton', repo: '', branch: 'main', root: '', runtime: 'fpm', database: null, redis: false, queue: false, scheduler: true })
const has = (k) => props.snap.databases.some((d) => d.kind === k)
const dbs = computed(() => [[null, t('ns.none'), true], ['mysql', 'MySQL', has('my_sql')], ['mariadb', 'MariaDB', has('maria_db')], ['postgres', 'PostgreSQL', has('postgres')]])
const laravel = computed(() => f.value.kind === 'laravel')
const anyDb = computed(() => dbs.value.slice(1).some((d) => d[2]))
const error = ref('')

function submit() {
  error.value = ''
  const v = f.value
  const domain = v.domain.trim().toLowerCase()
  if (!/^[a-z0-9.-]+\.[a-z]{2,}$/.test(domain)) { error.value = t('ns.domain'); return }
  if (v.code === 'github' && !/^[\w.-]+\/[\w.-]+$/.test(v.repo.trim())) { error.value = t('ns.repo_hint'); return }
  const args = { server: props.name, domain, kind: v.kind, web_root: v.root.trim() || (laravel.value ? 'public' : '.') }
  if (v.code === 'github') Object.assign(args, { repo: v.repo.trim(), branch: v.branch.trim() || 'main' })
  if (laravel.value) Object.assign(args, { runtime: v.runtime, database: v.database || undefined, redis: v.redis && has('redis'), queue: v.queue, scheduler: v.scheduler })
  ask('create_site', args, {
    title: `${t('ns.create')}: ${domain}`,
    details: [`${laravel.value ? 'Laravel' : t('ns.static')} · ${laravel.value ? (v.runtime === 'octane' ? t('ns.octane') : t('ns.fpm')) : ''}`, ...(v.database ? [`${t('ns.database')}: ${v.database}`] : [])],
    after: async () => { await rediscover(props.name); router.push(`/servers/${encodeURIComponent(props.name)}/sites/${encodeURIComponent(domain)}`) },
  })
}
</script>

<template>
  <div class="stack" style="max-width:900px">
    <router-link :to="`/servers/${encodeURIComponent(name)}/sites`" class="dim" style="text-decoration:none">← {{ t('sites.back') }}</router-link>
    <div class="card" style="gap:12px">
      <h3>{{ t('ns.title') }}</h3>
      <div class="dim">{{ t('ns.lead') }}</div>
      <div class="frow"><span>{{ t('ns.domain') }}</span><input v-model="f.domain" placeholder="shop.example.com" style="max-width:380px"></div>
      <div class="frow"><span>{{ t('ns.kind') }}</span><div class="seg"><button :class="{ on: f.kind === 'laravel' }" @click="f.kind = 'laravel'">Laravel</button><button :class="{ on: f.kind === 'static' }" @click="f.kind = 'static'">{{ t('ns.static') }}</button></div></div>
      <div class="frow"><span>{{ t('ns.source') }}</span><div class="seg"><button :class="{ on: f.code === 'skeleton' }" @click="f.code = 'skeleton'">{{ laravel ? t('ns.skeleton') : t('ns.placeholder') }}</button><button :class="{ on: f.code === 'github' }" @click="f.code = 'github'">{{ t('ns.github') }}</button></div></div>
      <template v-if="f.code === 'github'">
        <div class="frow"><span /><div class="row"><Brand name="github" :size="16" /><input v-model="f.repo" placeholder="owner/repo" style="width:260px"><input v-model="f.branch" placeholder="main" style="width:140px"></div></div>
        <div class="frow"><span /><span class="faint xs">{{ t('ns.repo_hint') }}</span></div>
      </template>
      <div class="frow"><span>{{ t('ns.root') }}</span><input v-model="f.root" :placeholder="laravel ? 'public' : '.'" style="width:200px"></div>
      <div v-if="laravel" class="frow"><span>{{ t('ns.runtime') }}</span><div class="seg"><button :class="{ on: f.runtime === 'fpm' }" @click="f.runtime = 'fpm'">{{ t('ns.fpm') }}</button><button :class="{ on: f.runtime === 'octane' }" @click="f.runtime = 'octane'">{{ t('ns.octane') }}</button></div></div>
    </div>
    <div class="card" style="gap:12px">
      <h3>{{ t('ns.services') }}</h3>
      <template v-if="laravel">
        <div class="frow"><span>{{ t('ns.database') }}</span><div class="seg"><button v-for="[v, label, ok] in dbs" :key="label" :class="{ on: f.database === v }" :disabled="!ok" @click="f.database = v">{{ label }}</button></div></div>
        <div v-if="!anyDb" class="faint xs">{{ t('ns.not_installed', { what: 'MySQL / MariaDB / PostgreSQL' }) }}</div>
        <div class="frow"><span /><div class="stack" style="gap:8px">
          <label class="row" style="gap:8px"><input v-model="f.redis" type="checkbox" :disabled="!has('redis')" style="width:auto">{{ t('ns.redis') }}</label>
          <span v-if="!has('redis')" class="faint xs">{{ t('ns.not_installed', { what: 'Redis' }) }}</span>
          <label class="row" style="gap:8px"><input v-model="f.queue" type="checkbox" style="width:auto">{{ t('ns.queue') }}</label>
          <label class="row" style="gap:8px"><input v-model="f.scheduler" type="checkbox" style="width:auto">{{ t('ns.scheduler') }}</label>
        </div></div>
      </template>
      <div v-else class="faint">—</div>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <div><button class="btn primary" @click="submit"><Icon name="rocket" :size="14" />{{ t('ns.create') }}</button></div>
  </div>
</template>

<style>
.frow { display: grid; grid-template-columns: 160px 1fr; gap: 16px; align-items: center; min-height: 38px; }
.frow > span:first-child { color: var(--dim); }
.seg { display: inline-flex; flex-wrap: wrap; gap: 2px; padding: 2px; border-radius: 9px; border: 1px solid var(--line); background: var(--panel); justify-self: start; }
.seg button { border: 0; background: none; padding: 5px 12px; border-radius: 7px; cursor: pointer; color: var(--dim); }
.seg button:hover:not(:disabled) { background: var(--hover); }
.seg button.on { background: var(--accent); color: var(--accent-text); }
.seg button:disabled { opacity: .45; cursor: default; }
@media (max-width: 700px) { .frow { grid-template-columns: 1fr; gap: 6px; } }
</style>
