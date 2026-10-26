<script setup>
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { api, signIn } from '../api.js'
import { t } from '../i18n.js'

const router = useRouter()
const setup = ref(false)
const form = ref({ email: '', password: '', name: '', setup_token: '' })
const error = ref('')
const busy = ref(false)

onMounted(async () => {
  try { setup.value = (await api('/setup')).needed } catch { /* offline */ }
})

async function submit() {
  error.value = ''
  busy.value = true
  try {
    const r = setup.value ? await api('/setup', { method: 'POST', body: form.value }) : await api('/login', { method: 'POST', body: { email: form.value.email, password: form.value.password } })
    signIn(r.token, r.user)
    router.push('/')
  } catch (e) {
    error.value = e.message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="center">
    <form class="card login" @submit.prevent="submit">
      <div class="row" style="gap:10px;margin-bottom:6px"><img src="/logo.svg" width="32" height="32" alt=""><h2 style="margin:0">{{ setup ? t('setup_title') : 'Sesame Cloud' }}</h2></div>
      <label v-if="setup">{{ t('setup_code') }}<input v-model="form.setup_token" class="mono" required></label>
      <label v-if="setup">{{ t('name') }}<input v-model="form.name" required></label>
      <label>{{ t('email') }}<input v-model="form.email" type="email" autocomplete="username" required></label>
      <label>{{ t('password') }}<input v-model="form.password" type="password" :autocomplete="setup ? 'new-password' : 'current-password'" :minlength="setup ? 10 : 1" required></label>
      <p v-if="error" class="error">{{ error }}</p>
      <button class="btn primary" :disabled="busy">{{ setup ? t('create_owner') : t('signin') }}</button>
    </form>
  </div>
</template>

<style scoped>
.center { min-height: 100vh; display: grid; place-items: center; padding: 16px; }
.login { width: min(380px, 100%); display: grid; gap: 12px; }
label { display: grid; gap: 4px; font-size: 13px; color: var(--dim); }
</style>
