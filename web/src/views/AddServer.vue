<script setup>
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { t } from '../i18n.js'
import { runNow, run } from '../runner.js'
import { loadServers } from '../store.js'
import Icon from '../components/Icon.vue'

const router = useRouter()
const f = ref({ target: '', name: '', key: '', password: '' })
async function submit() {
  const v = f.value
  const name = v.name.trim() || v.target.split('@').pop().split(':')[0]
  await runNow('add_server', { name, target: v.target.trim(), key_path: v.key.trim() || undefined, password: v.password || undefined }, { title: `${t('add.title')}: ${name}` })
  if (run.status === 'done') {
    await loadServers()
    router.push(`/servers/${encodeURIComponent(name)}/overview`)
  }
}
</script>

<template>
  <form class="card" style="max-width:640px;gap:14px" @submit.prevent="submit">
    <h3>{{ t('add.title') }}</h3>
    <div class="dim">{{ t('add.lead') }}</div>
    <label class="field">{{ t('add.address') }}<input v-model="f.target" placeholder="root@203.0.113.10" required><span class="faint xs">{{ t('add.address_hint') }}</span></label>
    <label class="field">{{ t('add.name') }} <span class="faint">{{ t('add.optional') }}</span><input v-model="f.name" placeholder="prod"><span class="faint xs">{{ t('add.name_hint') }}</span></label>
    <label class="field">{{ t('add.key') }} <span class="faint">{{ t('add.optional') }}</span><input v-model="f.key" :placeholder="t('add.key_placeholder')"></label>
    <label class="field">{{ t('add.password') }} <span class="faint">{{ t('add.optional') }}</span><input v-model="f.password" type="password" autocomplete="off" :placeholder="t('add.password_placeholder')"><span class="faint xs">{{ t('add.password_hint') }}</span></label>
    <div class="row"><button class="btn primary"><Icon name="plus" :size="14" />{{ t('add.submit') }}</button><router-link to="/" class="btn ghost">{{ t('add.cancel') }}</router-link></div>
  </form>
</template>

<style>
.field { display: grid; gap: 5px; color: var(--dim); }
.field input { color: var(--text); }
</style>
