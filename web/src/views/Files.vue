<script setup>
import { computed, onMounted, ref } from 'vue'
import { tool } from '../api.js'
import { t } from '../i18n.js'
import { ask, runNow } from '../runner.js'
import Icon from '../components/Icon.vue'
import { bytes } from '../sections.js'

const props = defineProps({ name: String, domain: String })
const path = ref('/')
const list = ref(null)
const error = ref('')
const filter = ref('')
const folder = ref('')
const editing = ref(null)
const loading = ref(false)
const saved = ref(false)
const base = () => ({ server: props.name, domain: props.domain })
const join = (dir, n) => `${dir.replace(/\/$/, '')}/${n}`
const parts = computed(() => path.value.split('/').filter(Boolean))

async function cd(p) {
  error.value = ''
  loading.value = true
  try { list.value = await tool('files_list', { ...base(), path: p }); path.value = p } catch (e) { error.value = e.message } finally { loading.value = false }
}
onMounted(() => cd('/'))
const entries = computed(() => {
  const q = filter.value.trim().toLowerCase()
  return (list.value?.entries || []).filter((e) => !q || e.name.toLowerCase().includes(q)).sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name))
})

async function open(e) {
  const p = join(path.value, e.name)
  if (e.dir) return cd(p)
  try { const r = await tool('files_read', { ...base(), path: p }); editing.value = { path: p, text: r.text }; saved.value = false } catch (err) { error.value = err.message }
}
async function save() {
  await runNow('files_write', { ...base(), path: editing.value.path, content: editing.value.text }, { title: `${t('files.save')}: ${editing.value.path}` })
  saved.value = true
}
async function download(e) {
  try {
    const r = await tool('files_read', { ...base(), path: join(path.value, e.name) })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(new Blob([r.text], { type: 'text/plain' }))
    a.download = e.name
    a.click()
  } catch (err) { error.value = err.message }
}
function mkdir() {
  const n = folder.value.trim()
  if (!n || n.includes('/')) return
  ask('files_mkdir', { ...base(), path: join(path.value, n) }, { title: `${t('files.new_folder')}: ${n}`, after: () => { folder.value = ''; return cd(path.value) } })
}
function rename(e) {
  const to = prompt(e.name, e.name)
  if (!to || to === e.name) return
  ask('files_rename', { ...base(), from: join(path.value, e.name), to: join(path.value, to) }, { title: `${e.name} → ${to}`, after: () => cd(path.value) })
}
function remove(e) {
  ask('files_remove', { ...base(), path: join(path.value, e.name) }, { title: t('files.confirm_delete', { name: e.name }), risk: 'destructive', after: () => cd(path.value) })
}
async function upload(ev) {
  for (const file of ev.target.files) {
    const text = await file.text()
    await runNow('files_write', { ...base(), path: join(path.value, file.name), content: text }, { title: `${t('files.upload')}: ${file.name}` })
  }
  ev.target.value = ''
  cd(path.value)
}
const when = (s) => (s ? new Date(s * 1000).toISOString().slice(0, 16).replace('T', ' ') : '')
</script>

<template>
  <div class="stack">
    <router-link :to="`/servers/${encodeURIComponent(name)}/sites/${encodeURIComponent(domain)}`" class="dim" style="text-decoration:none">← {{ domain }}</router-link>
    <div class="head"><div><h1>{{ t('files.title') }}</h1><div class="dim">{{ t('files.lead') }}</div></div></div>
    <div class="card">
      <div class="row wrap">
        <a class="link" @click="cd('/')">{{ list?.root || domain }}</a>
        <template v-for="(p, i) in parts" :key="i"><span class="faint">/</span><a class="link" @click="cd('/' + parts.slice(0, i + 1).join('/'))">{{ p }}</a></template>
        <div class="grow" />
        <span v-if="list?.owner" class="faint xs mono">{{ list.owner }}</span>
      </div>
      <div class="row wrap">
        <div class="search" style="width:220px"><Icon name="search" :size="14" class="faint" /><input v-model="filter" :placeholder="t('files.filter')"></div>
        <input v-model="folder" :placeholder="t('files.new_folder')" style="width:180px" @keyup.enter="mkdir">
        <button class="btn" @click="mkdir"><Icon name="plus" :size="14" />{{ t('files.new_folder') }}</button>
        <label class="btn"><Icon name="upload" :size="14" />{{ t('files.upload') }}<input type="file" multiple hidden @change="upload"></label>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
      <p v-if="loading" class="dim">{{ t('cf.loading') }}</p>
      <div v-if="list && !entries.length" class="dim">{{ t('files.empty') }}</div>
      <table v-else-if="list">
        <thead><tr><th>{{ t('db.name') }}</th><th>{{ t('files.size') }}</th><th>{{ t('files.modified') }}</th><th /></tr></thead>
        <tbody>
          <tr v-if="path !== '/'"><td colspan="4"><a class="link" @click="cd('/' + parts.slice(0, -1).join('/'))">..</a></td></tr>
          <tr v-for="e in entries" :key="e.name">
            <td><a class="row link" style="color:var(--text)" @click="open(e)"><Icon :name="e.dir ? 'folder' : 'file'" :size="14" :style="{ color: e.dir ? 'var(--accent)' : 'var(--dim)' }" />{{ e.name }}</a></td>
            <td class="dim">{{ e.dir ? '' : bytes(e.size) }}</td>
            <td class="faint xs">{{ when(e.modified) }} <span class="mono">{{ e.mode }}</span></td>
            <td style="text-align:right;white-space:nowrap">
              <button v-if="!e.dir" class="btn ghost xs" :title="t('files.download')" @click="download(e)"><Icon name="download" :size="12" /></button>
              <button class="btn ghost xs" title="Rename" @click="rename(e)">✎</button>
              <button class="btn ghost xs" :title="t('files.delete')" @click="remove(e)"><Icon name="trash" :size="12" /></button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="editing" class="card">
      <div class="row between"><span class="mono">{{ editing.path }}</span><span class="row"><span v-if="saved" class="pill ok">{{ t('files.saved') }}</span><button class="btn primary" @click="save">{{ t('files.save') }}</button><button class="btn" @click="editing = null">{{ t('files.close') }}</button></span></div>
      <textarea v-model="editing.text" spellcheck="false" style="min-height:420px;resize:vertical" @input="saved = false" />
    </div>
  </div>
</template>
