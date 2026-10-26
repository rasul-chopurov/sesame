<script setup>
import { computed, nextTick, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { t } from '../i18n.js'
import { state, rediscover, setTheme } from '../store.js'
import { SECTIONS } from '../sections.js'
import Icon from './Icon.vue'

const emit = defineEmits(['close'])
const router = useRouter()
const q = ref('')
const sel = ref(0)
const input = ref(null)
onMounted(() => nextTick(() => input.value?.focus()))

const items = computed(() => {
  const out = []
  for (const s of state.servers) {
    const base = `/servers/${encodeURIComponent(s.name)}`
    for (const sec of SECTIONS) out.push({ icon: sec.icon, label: `${s.name} › ${t(sec.key)}`, hint: t('palette.go'), go: `${base}/${sec.slug}` })
    out.push({ icon: 'plus', label: `${s.name} › ${t('ns.title')}`, hint: t('palette.go'), go: `${base}/new-site` })
    out.push({ icon: 'refresh', label: `${s.name} › ${t('server.rediscover')}`, hint: t('palette.run'), run: () => rediscover(s.name) })
    for (const site of state.snaps[s.name]?.sites || []) {
      const d = site.domains[0]
      if (!d || d === '*') continue
      out.push({ icon: 'globe', label: d, hint: s.name, go: `${base}/sites/${encodeURIComponent(d)}` })
      out.push({ icon: 'folder', label: `${d} › ${t('files.title')}`, hint: s.name, go: `${base}/files/${encodeURIComponent(d)}` })
    }
  }
  out.push({ icon: 'cloud', label: 'Cloudflare', hint: t('palette.go'), go: '/cloudflare' })
  out.push({ icon: 'plus', label: t('nav.add_server'), hint: t('palette.go'), go: '/add-server' })
  out.push({ icon: 'settings', label: t('nav.settings'), hint: t('palette.go'), go: '/settings' })
  out.push({ icon: 'bot', label: t('web.approvals'), hint: t('palette.go'), go: '/approvals' })
  out.push({ icon: 'list', label: t('web.audit'), hint: t('palette.go'), go: '/audit' })
  for (const [v, k, i] of [['light', 'palette.light', 'sun'], ['dark', 'palette.dark', 'moon'], ['system', 'palette.system', 'monitor']]) {
    out.push({ icon: i, label: t(k), hint: t('palette.run'), run: () => setTheme(v) })
  }
  const words = q.value.trim().toLowerCase().split(/\s+/).filter(Boolean)
  return words.length ? out.filter((it) => words.every((w) => `${it.label} ${it.hint}`.toLowerCase().includes(w))) : out
})

function exec(it) {
  if (!it) return
  emit('close')
  if (it.go) router.push(it.go)
  else it.run()
}
function key(e) {
  const n = items.value.length
  if (!n) return
  if (e.key === 'ArrowDown') { sel.value = (sel.value + 1) % n; e.preventDefault() }
  if (e.key === 'ArrowUp') { sel.value = (sel.value - 1 + n) % n; e.preventDefault() }
  if (e.key === 'Enter') exec(items.value[sel.value])
}
</script>

<template>
  <div class="scrim pal-scrim" @click.self="emit('close')">
    <div class="palette card">
      <input ref="input" v-model="q" :placeholder="t('palette.placeholder')" @input="sel = 0" @keydown="key">
      <div class="pal-list">
        <div v-for="(it, i) in items.slice(0, 40)" :key="it.label + i" class="pal-item" :class="{ on: i === sel }" @mouseenter="sel = i" @click="exec(it)">
          <Icon :name="it.icon" :size="15" :style="{ color: i === sel ? 'var(--accent)' : 'var(--dim)' }" />
          <span class="grow">{{ it.label }}</span><span class="faint xs">{{ it.hint }}</span>
        </div>
      </div>
      <div class="faint xs">{{ t('palette.keys') }}</div>
    </div>
  </div>
</template>

<style>
.pal-scrim { align-items: start; padding-top: 12vh; }
.palette { width: min(580px, 100%); background: var(--panel-solid); padding: 10px; }
.pal-list { max-height: 50vh; overflow-y: auto; display: grid; gap: 1px; }
.pal-item { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 7px; cursor: pointer; }
.pal-item.on { background: var(--hover); }
</style>
