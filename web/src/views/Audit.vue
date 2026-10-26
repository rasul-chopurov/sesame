<script setup>
import { onMounted, ref } from 'vue'
import { api } from '../api.js'
import { t } from '../i18n.js'

const list = ref([])
const error = ref('')
onMounted(async () => {
  try { list.value = await api('/audit?limit=200') } catch (e) { error.value = e.message }
})
</script>

<template>
  <div class="head" style="margin-bottom:16px"><h1>{{ t('web.audit') }}</h1></div>
  <p v-if="error" class="error">{{ error }}</p>
  <div class="card" style="overflow-x:auto">
    <table>
      <thead><tr><th>{{ t('web.when') }}</th><th>{{ t('web.server') }}</th><th>{{ t('web.who') }}</th><th>{{ t('web.what') }}</th><th></th></tr></thead>
      <tbody>
        <tr v-for="(a, i) in list" :key="i">
          <td class="mono faint">{{ a.at.replace('T', ' ').slice(0, 16) }}</td>
          <td>{{ a.server || '—' }}</td>
          <td class="mono">{{ a.actor }}</td>
          <td>{{ a.title }}</td>
          <td><span :class="['pill', a.ok ? 'ok' : 'bad']">{{ a.ok ? t('web.ok') : t('web.failed') }}</span></td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
