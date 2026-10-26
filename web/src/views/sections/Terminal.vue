<script setup>
import { onMounted, onUnmounted, ref } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { session } from '../../api.js'
import { t } from '../../i18n.js'
import { isAdmin } from '../../sections.js'
import Icon from '../../components/Icon.vue'

const props = defineProps({ name: String, snap: Object })
import { state } from '../../store.js'
const user = (state.servers.find((s) => s.name === props.name)?.address || 'root@').split('@')[0]
const el = ref(null)
const status = ref('idle')
let term, fit, ws, ro

function connect() {
  ws?.close()
  term.reset()
  status.value = 'connecting'
  term.write(`\x1b[2m${t('term.connecting')}\x1b[0m\r\n`)
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  ws = new WebSocket(`${proto}://${location.host}/api/terminal?server=${encodeURIComponent(props.name)}&cols=${term.cols}&rows=${term.rows}`, ['sesame', session.token])
  ws.binaryType = 'arraybuffer'
  ws.onopen = () => { status.value = 'open'; term.focus() }
  ws.onmessage = (e) => term.write(typeof e.data === 'string' ? e.data : new Uint8Array(e.data))
  ws.onclose = () => { status.value = 'closed'; term.write(`\r\n\x1b[2m${t('term.closed')}\x1b[0m\r\n`) }
}

onMounted(() => {
  const dark = document.documentElement.dataset.theme === 'dark'
  term = new Terminal({ fontFamily: 'JetBrains Mono, ui-monospace, Menlo, monospace', fontSize: 13, cursorBlink: true, theme: dark ? { background: '#14111c', foreground: '#ece6dc' } : { background: '#fbf8f3', foreground: '#221d2e', cursor: '#d9601a', selectionBackground: '#f3c9a5' } })
  fit = new FitAddon()
  term.loadAddon(fit)
  term.open(el.value)
  fit.fit()
  term.onData((d) => ws?.readyState === 1 && ws.send(d))
  term.onResize(({ cols, rows }) => ws?.readyState === 1 && ws.send(`\u0001resize:${cols}x${rows}`))
  ro = new ResizeObserver(() => fit.fit())
  ro.observe(el.value)
  if (isAdmin(session.user)) connect()
})
onUnmounted(() => { ws?.close(); ro?.disconnect(); term?.dispose() })
</script>

<template>
  <div class="stack">
    <div class="row between">
      <div class="dim">{{ t('term.lead', { user }) }}</div>
      <button v-if="isAdmin(session.user)" class="btn" @click="connect"><Icon name="refresh" :size="14" />{{ status === 'open' ? t('term.reconnect') : t('term.connect') }}</button>
    </div>
    <div v-if="!isAdmin(session.user)" class="card dim">{{ t('web.read_only') }}</div>
    <div ref="el" class="term panel" />
  </div>
</template>

<style>
.term { height: calc(100vh - 230px); min-height: 320px; padding: 8px; overflow: hidden; }
</style>
