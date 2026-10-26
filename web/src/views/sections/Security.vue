<script setup>
import { computed } from 'vue'
import { t } from '../../i18n.js'
const props = defineProps({ name: String, snap: Object })
const ports = computed(() => [...props.snap.ports].sort((a, b) => Number(b.public) - Number(a.public) || a.port - b.port))
const days = (d) => (d ? Math.round((new Date(d) - Date.now()) / 86400000) : null)
const certColor = (d) => { const n = days(d); return n === null ? '' : n < 0 ? 'bad' : n < 14 ? 'warn' : 'ok' }
</script>

<template>
  <div class="stack">
    <div class="card">
      <h3>{{ t('sec.firewall') }}</h3>
      <div class="kv"><span>ufw</span><span><span :class="['pill', snap.firewall.ufw === 'active' ? 'ok' : 'warn']">{{ snap.firewall.ufw || t('sec.not_installed') }}</span></span></div>
      <div class="kv"><span>fail2ban</span><span>{{ snap.firewall.fail2ban_jails.length ? snap.firewall.fail2ban_jails.join(', ') : t('sec.no_jails') }}</span></div>
    </div>
    <div class="card">
      <h3>{{ t('sec.ports') }}</h3>
      <table>
        <tbody>
          <tr v-for="p in ports" :key="p.address + p.port">
            <td style="width:100px"><span :class="['pill', p.public ? 'warn' : 'ok']">{{ p.public ? t('sec.public') : t('sec.local') }}</span></td>
            <td class="mono" style="width:200px">{{ p.address }}:{{ p.port }}</td><td class="dim">{{ p.process }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="card">
      <h3>{{ t('sec.certs') }}</h3>
      <div v-if="!snap.certificates.length" class="dim">{{ t('sec.no_certs') }}</div>
      <div v-for="c in snap.certificates" :key="c.path" class="row wrap" style="min-height:28px">
        <span class="dot" :style="{ background: `var(--${certColor(c.not_after) || 'faint'})` }" />
        <span class="grow">{{ c.names.join(', ') || c.subject }}</span>
        <span class="faint xs">{{ c.issuer }}</span>
        <span v-if="c.not_after" :class="['pill', certColor(c.not_after)]">{{ t('sec.expires', { date: c.not_after.slice(0, 10) }) }}</span>
        <span class="mono faint xs" style="width:100%">{{ c.path }}</span>
      </div>
    </div>
  </div>
</template>
