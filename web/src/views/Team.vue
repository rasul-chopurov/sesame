<script setup>
import { onMounted, ref } from 'vue'
import { api, session } from '../api.js'
import { t } from '../i18n.js'

const users = ref([])
const error = ref('')
const person = ref({ email: '', name: '', role: 'developer', password: '' })
const agent = ref({ name: '', role: 'developer' })
const newToken = ref('')

async function load() {
  try { users.value = await api('/users') } catch (e) { error.value = e.message }
}
async function addPerson() {
  error.value = ''
  try { await api('/users', { method: 'POST', body: person.value }); person.value = { email: '', name: '', role: 'developer', password: '' }; load() } catch (e) { error.value = e.message }
}
async function addAgent() {
  error.value = ''
  try { const r = await api('/agents', { method: 'POST', body: agent.value }); newToken.value = r.token; agent.value = { name: '', role: 'developer' }; load() } catch (e) { error.value = e.message }
}
async function setRole(u, role) {
  try { await api(`/users/${u.id}`, { method: 'PATCH', body: { role } }); load() } catch (e) { error.value = e.message; load() }
}
async function remove(u) {
  if (!window.confirm(`${t('web.remove')} ${u.name || u.email}?`)) return
  try { await api(`/users/${u.id}`, { method: 'DELETE' }); load() } catch (e) { error.value = e.message }
}
onMounted(load)
const roles = () => (session.user?.role === 'owner' ? ['viewer', 'developer', 'admin'] : ['viewer', 'developer'])
</script>

<template>
  <div class="head" style="margin-bottom:16px"><h1>{{ t('web.team') }}</h1></div>
  <p v-if="error" class="error">{{ error }}</p>
  <div class="stack">
    <div class="card" style="overflow-x:auto">
      <table>
        <thead><tr><th>{{ t('web.name') }}</th><th>{{ t('web.email') }}</th><th>{{ t('web.role') }}</th><th></th></tr></thead>
        <tbody>
          <tr v-for="u in users" :key="u.id">
            <td>{{ u.name }} <span v-if="u.agent" class="pill">agent</span></td>
            <td class="mono dim">{{ u.agent ? '—' : u.email }}</td>
            <td>
              <span v-if="u.role === 'owner' || u.id === session.user?.id" class="pill">{{ u.role }}</span>
              <select v-else :value="u.role" style="width:auto" @change="setRole(u, $event.target.value)">
                <option v-for="r in roles()" :key="r" :value="r">{{ r }}</option>
              </select>
            </td>
            <td><button v-if="u.role !== 'owner' && u.id !== session.user?.id" class="btn sm" @click="remove(u)">{{ t('web.remove') }}</button></td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="grid">
      <form class="card form" @submit.prevent="addPerson">
        <h3>{{ t('web.add_person') }}</h3>
        <input v-model="person.name" :placeholder="t('web.name')" required>
        <input v-model="person.email" type="email" :placeholder="t('web.email')" required>
        <input v-model="person.password" type="password" minlength="10" :placeholder="t('web.password')" autocomplete="new-password" required>
        <select v-model="person.role"><option v-for="r in roles()" :key="r" :value="r">{{ r }}</option></select>
        <button class="btn primary">{{ t('web.add_person') }}</button>
      </form>
      <form class="card form" @submit.prevent="addAgent">
        <h3>{{ t('web.add_agent') }}</h3>
        <input v-model="agent.name" placeholder="claude-code" required>
        <select v-model="agent.role"><option value="viewer">viewer</option><option value="developer">developer</option></select>
        <button class="btn primary">{{ t('web.add_agent') }}</button>
        <template v-if="newToken">
          <p class="dim" style="margin:0">{{ t('web.token_once') }}</p>
          <input class="mono" :value="newToken" readonly @focus="$event.target.select()">
        </template>
      </form>
    </div>
  </div>
</template>

<style scoped>
.form { display: grid; gap: 8px; align-content: start; }
</style>
