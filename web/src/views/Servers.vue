<script setup>
import { onMounted, ref } from 'vue'
import { tool } from '../api.js'
import { t } from '../i18n.js'

const servers = ref([])
const error = ref('')
onMounted(async () => {
  try { servers.value = await tool('list_servers') } catch (e) { error.value = e.message }
})
</script>

<template>
  <div class="page-title"><h1>{{ t('servers') }}</h1></div>
  <p v-if="error" class="error">{{ error }}</p>
  <p v-else-if="!servers.length" class="dim">{{ t('no_servers') }}</p>
  <div class="grid">
    <router-link v-for="s in servers" :key="s.name" :to="`/servers/${encodeURIComponent(s.name)}`" class="card server">
      <div class="row"><span class="dot" :style="{ background: s.discovered_at ? 'var(--ok)' : 'var(--faint)' }"></span><b>{{ s.name }}</b></div>
      <div class="mono dim">{{ s.address }}</div>
      <div class="faint">{{ s.os || '—' }} · {{ t('sites') }}: {{ s.sites ?? '—' }}</div>
      <div v-if="s.discovered_at" class="faint" style="font-size:12px">{{ t('discovered') }} {{ s.discovered_at.replace('T', ' ').slice(0, 16) }}</div>
    </router-link>
  </div>
</template>

<style scoped>
.server { text-decoration: none; display: grid; gap: 6px; }
.server:hover { border-color: var(--accent); }
</style>
