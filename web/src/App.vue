<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api, session, signOut } from './api.js'
import { t, lang, langs, setLang } from './i18n.js'
import { state, loadServers, loadIntegrations, pollApprovals, setTheme } from './store.js'
import { SECTIONS, isAdmin, short } from './sections.js'
import Icon from './components/Icon.vue'
import RunDialog from './components/RunDialog.vue'
import Palette from './components/Palette.vue'

const route = useRoute()
const router = useRouter()
const filter = ref('')
const menu = ref(false)
const palette = ref(false)
let timer

async function boot() {
  if (!session.token) return
  try { session.user = await api('/me') } catch { return }
  await Promise.all([loadServers().catch(() => {}), loadIntegrations(), pollApprovals()])
}
onMounted(() => {
  boot()
  timer = setInterval(pollApprovals, 4000)
  window.addEventListener('keydown', onKey)
})
onUnmounted(() => { clearInterval(timer); window.removeEventListener('keydown', onKey) })
watch(() => session.token, (tok) => (tok ? boot() : router.push('/login')))
watch(() => route.fullPath, () => { menu.value = false })

function onKey(e) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); palette.value = !palette.value }
  if (e.key === 'Escape') palette.value = false
}

const visible = computed(() => {
  const q = filter.value.trim().toLowerCase()
  return state.servers.filter((s) => {
    if (!q) return true
    if (s.name.toLowerCase().includes(q) || s.address.toLowerCase().includes(q)) return true
    return (state.snaps[s.name]?.sites || []).some((x) => x.domains.some((d) => d.includes(q)))
  })
})

const crumbs = computed(() => {
  const out = []
  const p = route.params
  if (p.name) {
    out.push(p.name)
    if (route.path.includes('/sites/') || route.path.includes('/files/')) out.push(t('nav.sites'), p.domain)
    else if (p.section === 'new-site') out.push(t('nav.sites'), t('ns.title'))
    else if (p.section) out.push(t(SECTIONS.find((s) => s.slug === p.section)?.key || p.section))
  } else if (route.path === '/add-server') out.push(t('nav.add_server'))
  else if (route.path === '/cloudflare') out.push('Cloudflare')
  else if (route.path === '/settings') out.push(t('nav.settings'))
  else if (route.path === '/approvals') out.push(t('web.approvals'))
  else if (route.path === '/audit') out.push(t('web.audit'))
  else if (route.path === '/team') out.push(t('web.team'))
  return out
})

const current = computed(() => (route.params.name ? state.servers.find((s) => s.name === route.params.name) : null))
const themeIcon = computed(() => ({ system: 'monitor', light: 'sun', dark: 'moon' })[state.theme])
function cycleTheme() { setTheme({ system: 'light', light: 'dark', dark: 'system' }[state.theme]) }
function cycleLang() { setLang(langs[(langs.indexOf(lang.value) + 1) % langs.length]) }
function out() { signOut(); router.push('/login') }
const sectionPath = (s, slug) => `/servers/${encodeURIComponent(s.name)}/${slug}`
</script>

<template>
  <router-view v-if="!session.token" />
  <div v-else class="shell">
    <header class="titlebar">
      <button class="btn ghost xs burger" @click="menu = !menu"><Icon name="menu" /></button>
      <router-link to="/" class="brand"><img src="/logo.svg" width="18" height="18" alt=""> Sesame</router-link>
      <template v-for="c in crumbs" :key="c"><span class="faint">/</span><span class="crumb">{{ c }}</span></template>
      <div class="grow" />
      <button class="tool" :title="t('palette.placeholder')" @click="palette = true"><Icon name="search" :size="14" /><span class="faint xs">⌘K</span></button>
      <router-link v-if="state.approvals" to="/approvals" class="tool"><span class="pill warn">{{ state.approvals }}</span></router-link>
      <button class="tool b xs dim" @click="cycleLang">{{ lang.toUpperCase() }}</button>
      <button class="tool dim" @click="cycleTheme"><Icon :name="themeIcon" :size="15" /></button>
      <router-link to="/settings" class="tool dim"><Icon name="settings" :size="15" /></router-link>
    </header>

    <aside :class="{ open: menu }">
      <div class="filter"><Icon name="search" :size="14" class="faint" /><input v-model="filter" :placeholder="t('nav.filter')"></div>
      <div class="group"><span>{{ t('nav.servers') }}</span><router-link v-if="isAdmin(session.user)" to="/add-server" class="faint"><Icon name="plus" :size="14" /></router-link></div>
      <template v-for="s in visible" :key="s.name">
        <router-link :to="sectionPath(s, 'overview')" class="srv" :class="{ on: route.params.name === s.name }">
          <Icon name="server" :size="15" /><span class="grow">{{ s.name }}</span>
          <span class="dot" :style="{ background: state.errors[s.name] ? 'var(--bad)' : s.discovered_at ? 'var(--ok)' : 'var(--faint)' }" />
        </router-link>
        <div v-if="route.params.name === s.name && state.snaps[s.name]" class="sections">
          <router-link v-for="sec in SECTIONS" :key="sec.slug" :to="sectionPath(s, sec.slug)" :class="{ on: route.params.section === sec.slug || (sec.slug === 'sites' && (route.path.includes('/sites/') || route.path.includes('/files/') || route.params.section === 'new-site')) }">
            <Icon :name="sec.icon" :size="14" /><span class="grow">{{ t(sec.key) }}</span>
            <span v-if="sec.count" class="faint xs">{{ sec.count(state.snaps[s.name]) }}</span>
          </router-link>
        </div>
      </template>
      <p v-if="!state.servers.length" class="faint xs" style="padding:0 10px">{{ t('web.no_servers') }}</p>
      <div class="group"><span>{{ t('nav.integrations') }}</span></div>
      <router-link to="/cloudflare" class="srv" :class="{ on: route.path === '/cloudflare' }">
        <img src="/brands/cloudflare.svg" width="15" height="15" alt=""><span class="grow">Cloudflare</span>
        <span class="dot" :style="{ background: state.integrations.cloudflare?.connected ? 'var(--ok)' : 'var(--faint)' }" />
      </router-link>
      <div class="group"><span>Cloud</span></div>
      <router-link to="/approvals" class="srv"><Icon name="bot" :size="15" /><span class="grow">{{ t('web.approvals') }}</span><span v-if="state.approvals" class="pill warn">{{ state.approvals }}</span></router-link>
      <router-link to="/audit" class="srv"><Icon name="list" :size="15" /><span class="grow">{{ t('web.audit') }}</span></router-link>
      <router-link v-if="isAdmin(session.user)" to="/team" class="srv"><Icon name="users" :size="15" /><span class="grow">{{ t('web.team') }}</span></router-link>
      <div class="me">
        <div class="grow"><div class="b">{{ session.user?.name || session.user?.email }}</div><div class="faint xs">{{ session.user?.role }}</div></div>
        <button class="btn xs" @click="out">{{ t('web.signout') }}</button>
      </div>
    </aside>

    <main @click="menu = false"><router-view :key="route.params.name + (route.params.section || '') + (route.params.domain || '')" /></main>

    <footer class="status">
      <template v-if="current">
        <span class="dot" :style="{ background: state.errors[current.name] ? 'var(--bad)' : 'var(--ok)' }" />
        <span class="mono">{{ current.address }}</span>
        <span v-if="state.snaps[current.name]">{{ t('server.discovered_at', { time: short(state.snaps[current.name].taken_at) }) }}</span>
        <span v-if="state.snaps[current.name]">{{ t('status.sites', { n: state.snaps[current.name].sites.length }) }}</span>
        <span v-if="state.snaps[current.name]">{{ t('status.processes', { n: state.snaps[current.name].supervisor.length }) }}</span>
      </template>
      <div class="grow" />
      <span class="mono">Sesame Cloud v0.1.0-alpha.1</span>
    </footer>
  </div>
  <RunDialog />
  <Palette v-if="palette" @close="palette = false" />
</template>

<style>
.shell { display: grid; grid-template-columns: 250px 1fr; grid-template-rows: 40px 1fr 26px; height: 100vh; }
.titlebar { grid-column: 1 / -1; display: flex; align-items: center; gap: 8px; padding: 0 10px 0 14px; border-bottom: 1px solid var(--line); font-size: 13px; }
.brand { display: flex; align-items: center; gap: 6px; font-weight: 700; text-decoration: none; }
.crumb { color: var(--dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 240px; }
.tool { display: inline-flex; align-items: center; gap: 5px; padding: 4px 7px; border-radius: 6px; border: 0; background: none; cursor: pointer; text-decoration: none; }
.tool:hover { background: var(--hover); }
aside { overflow-y: auto; padding: 10px; display: flex; flex-direction: column; gap: 2px; border-right: 1px solid var(--line); }
.filter { display: flex; align-items: center; gap: 6px; padding: 0 8px; border: 1px solid var(--line); border-radius: 7px; background: var(--panel-solid); margin-bottom: 8px; }
.filter input { border: 0; background: none; padding: 6px 0; outline: none; }
.group { display: flex; justify-content: space-between; align-items: center; font-size: 11px; letter-spacing: .07em; text-transform: uppercase; color: var(--dim); font-weight: 600; padding: 10px 8px 4px; }
.srv, .sections a { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 7px; text-decoration: none; color: var(--text); }
.srv:hover, .sections a:hover { background: var(--hover); }
.srv.on { background: var(--hover); font-weight: 600; }
.sections { display: grid; gap: 1px; padding-left: 14px; margin-bottom: 4px; }
.sections a { color: var(--dim); padding: 5px 8px; }
.sections a.on { background: var(--active); color: var(--text); }
.me { margin-top: auto; display: flex; align-items: center; gap: 8px; padding: 10px 6px 2px; border-top: 1px solid var(--line); }
main { overflow-y: auto; padding: 22px 26px 40px; min-width: 0; }
.status { grid-column: 1 / -1; display: flex; align-items: center; gap: 14px; padding: 0 12px; font-size: 11.5px; color: var(--dim); border-top: 1px solid var(--line); white-space: nowrap; overflow: hidden; }
.btn.burger { display: none; }
@media (max-width: 860px) {
  .shell { grid-template-columns: 1fr; }
  .btn.burger { display: inline-flex; }
  aside { position: fixed; top: 40px; bottom: 0; left: 0; width: 280px; z-index: 20; background: var(--panel-solid); transform: translateX(-105%); transition: transform .2s; }
  aside.open { transform: none; box-shadow: 0 0 50px rgb(0 0 0 / .3); }
  main { padding: 16px; }
  .status .mono:first-of-type ~ span { display: none; }
  .crumb { max-width: 120px; }
}
</style>
