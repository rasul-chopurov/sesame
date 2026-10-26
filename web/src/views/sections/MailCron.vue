<script setup>
import { t } from '../../i18n.js'
defineProps({ name: String, snap: Object })
</script>

<template>
  <div class="stack">
    <div class="card">
      <h3>{{ t('mail.title') }}</h3>
      <div class="kv"><span>{{ t('mail.server') }}</span><span>{{ snap.mail.servers.length ? snap.mail.servers.join(', ') : t('mail.none_local') }}</span></div>
      <div v-if="snap.mail.hostname" class="kv"><span>{{ t('mail.hostname') }}</span><span class="mono">{{ snap.mail.hostname }}</span></div>
      <div v-if="snap.mail.domains.length" class="kv"><span>{{ t('mail.domains') }}</span><span>{{ snap.mail.domains.join(', ') }}</span></div>
      <div v-if="snap.mail.external.length" class="kv"><span>{{ t('mail.apps_send') }}</span><span>{{ snap.mail.external.join(', ') }}</span></div>
    </div>
    <div class="card">
      <h3>{{ t('cron.title') }}</h3>
      <div v-if="!snap.cron.length" class="dim">{{ t('cron.none') }}</div>
      <table v-else>
        <tbody>
          <tr v-for="(c, i) in snap.cron" :key="i"><td class="dim" style="width:110px">{{ c.user }}</td><td class="mono" style="width:140px">{{ c.schedule }}</td><td class="mono" style="overflow-wrap:anywhere">{{ c.command }}</td></tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
