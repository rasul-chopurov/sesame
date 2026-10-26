<script setup>
import { onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { api, session, signOut } from './api.js'
import { t, lang, langs, setLang } from './i18n.js'
import { run, confirm, close } from './runner.js'

const router = useRouter()
const pending = ref(0)
const menu = ref(false)

async function loadMe() {
  if (!session.token) return
  try { session.user = await api('/me') } catch { /* signed out by api() */ }
}
async function poll() {
  if (!session.token) return
  try { pending.value = (await api('/approvals')).length } catch { /* ignore */ }
}
onMounted(() => { loadMe(); poll(); setInterval(poll, 5000) })
watch(() => session.token, (tok) => { if (tok) { loadMe(); poll() } else router.push('/login') })

function out() { signOut(); router.push('/login') }
const isAdmin = () => ['admin', 'owner'].includes(session.user?.role)
</script>

<template>
  <div v-if="!session.token"><router-view /></div>
  <div v-else class="shell">
    <aside :class="{ open: menu }">
      <div class="brand"><img src="/logo.svg" alt="" width="24" height="24"> Sesame <span class="pill">Cloud</span></div>
      <nav @click="menu = false">
        <router-link to="/">{{ t('servers') }}</router-link>
        <router-link to="/approvals">{{ t('approvals') }} <span v-if="pending" class="pill warn">{{ pending }}</span></router-link>
        <router-link to="/audit">{{ t('audit') }}</router-link>
        <router-link v-if="isAdmin()" to="/team">{{ t('team') }}</router-link>
      </nav>
      <div class="me">
        <div v-if="session.user"><b>{{ session.user.name || session.user.email }}</b><div class="faint">{{ session.user.role }}</div></div>
        <div class="row">
          <select :value="lang" @change="setLang($event.target.value)" style="width:auto">
            <option v-for="l in langs" :key="l" :value="l">{{ l.toUpperCase() }}</option>
          </select>
          <button class="btn sm" @click="out">{{ t('signout') }}</button>
        </div>
      </div>
    </aside>
    <main>
      <button class="btn sm burger" @click="menu = !menu">☰</button>
      <router-view />
    </main>
  </div>

  <div v-if="run.open" class="scrim" @click.self="run.status !== 'running' && run.status !== 'approval' && close()">
    <div class="dialog card">
      <div class="row" style="justify-content:space-between">
        <b>{{ run.title }}</b>
        <span :class="['pill', run.risk === 'destructive' ? 'bad' : 'warn']">{{ run.risk === 'destructive' ? t('destructive') : t('changes') }}</span>
      </div>
      <ul v-if="run.details.length" class="dim"><li v-for="d in run.details" :key="d">{{ d }}</li></ul>
      <template v-if="run.status === 'ask'">
        <div v-if="run.word" style="margin:10px 0">
          <div class="dim" style="margin-bottom:6px">{{ t('type_to_confirm', { word: run.word }) }}</div>
          <input v-model="run.typed" autofocus>
        </div>
        <div class="row" style="justify-content:flex-end;margin-top:12px">
          <button class="btn" @click="close">{{ t('cancel') }}</button>
          <button :class="['btn', run.risk === 'destructive' ? 'danger' : 'primary']" :disabled="run.word && run.typed.trim() !== run.word" @click="confirm">{{ t('confirm') }}</button>
        </div>
      </template>
      <p v-else-if="run.status === 'running'" class="dim">{{ t('running') }}</p>
      <p v-else-if="run.status === 'approval'" class="dim">{{ t('waiting_approval') }}</p>
      <template v-else>
        <p v-if="run.status === 'done'" class="pill ok">{{ t('done') }}</p>
        <p v-else class="error">{{ run.error }}</p>
        <pre v-if="run.output" class="mono out">{{ run.output }}</pre>
        <div class="row" style="justify-content:flex-end"><button class="btn" @click="close">OK</button></div>
      </template>
    </div>
  </div>
</template>

<style>
.shell { display: grid; grid-template-columns: 230px 1fr; min-height: 100vh; }
aside { border-right: 1px solid var(--line); padding: 16px; display: flex; flex-direction: column; gap: 18px; background: var(--panel); }
.brand { display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: 16px; }
nav { display: grid; gap: 2px; }
nav a { text-decoration: none; padding: 7px 10px; border-radius: 8px; color: var(--dim); display: flex; justify-content: space-between; }
nav a.router-link-exact-active, nav a:hover { background: var(--hover); color: var(--ink); }
.me { margin-top: auto; display: grid; gap: 8px; font-size: 14px; }
main { padding: 24px; min-width: 0; }
.btn.burger { display: none; margin-bottom: 12px; }
.scrim { position: fixed; inset: 0; background: rgb(0 0 0 / .35); display: grid; place-items: center; padding: 16px; z-index: 10; }
.dialog { width: min(520px, 100%); }
.out { max-height: 260px; overflow: auto; background: var(--hover); padding: 10px; border-radius: 8px; white-space: pre-wrap; }
@media (max-width: 760px) {
  .shell { grid-template-columns: 1fr; }
  aside { position: fixed; inset: 0 30% 0 0; z-index: 5; transform: translateX(-105%); transition: transform .2s; }
  aside.open { transform: none; box-shadow: 0 0 40px rgb(0 0 0 / .3); }
  .btn.burger { display: inline-flex; }
  main { padding: 16px; }
}
</style>
