<script setup>
import { onMounted, onUnmounted, ref } from 'vue'
import { api, session } from '../api.js'
import { t } from '../i18n.js'

const list = ref([])
const error = ref('')
let timer
async function load() {
  try { list.value = await api('/approvals') } catch (e) { error.value = e.message }
}
async function decide(id, d) {
  try { await api(`/approvals/${id}/${d}`, { method: 'POST' }); load() } catch (e) { error.value = e.message }
}
onMounted(() => { load(); timer = setInterval(load, 3000) })
onUnmounted(() => clearInterval(timer))
const canDecide = () => ['admin', 'owner'].includes(session.user?.role)
</script>

<template>
  <div class="head" style="margin-bottom:16px"><h1>{{ t('web.approvals') }}</h1></div>
  <p v-if="error" class="error">{{ error }}</p>
  <p v-if="!list.length" class="dim">{{ t('web.nothing_pending') }}</p>
  <div class="stack">
    <div v-for="a in list" :key="a.id" class="card" :style="{ borderColor: a.risk === 'destructive' ? 'var(--bad)' : 'var(--accent)' }">
      <div class="row" style="justify-content:space-between;flex-wrap:wrap">
        <div><b>{{ a.actor.replace(/^agent:|^user:/, '') }}</b> <span class="dim">· {{ a.server || '' }}</span></div>
        <span :class="['pill', a.risk === 'destructive' ? 'bad' : 'warn']">{{ a.risk === 'destructive' ? t('job.risk_destructive') : t('job.risk_change') }}</span>
      </div>
      <div style="margin:8px 0;font-weight:500">{{ a.title }}</div>
      <ul class="dim"><li v-for="d in a.details" :key="d">{{ d }}</li></ul>
      <div v-if="canDecide()" class="row" style="justify-content:flex-end">
        <button class="btn" @click="decide(a.id, 'deny')">{{ t('approve.deny') }}</button>
        <button :class="['btn', a.risk === 'destructive' ? 'danger' : 'primary']" @click="decide(a.id, 'allow')">{{ t('approve.allow') }}</button>
      </div>
    </div>
  </div>
</template>
