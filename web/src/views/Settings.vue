<script setup>
import { reactive } from 'vue'
import { session } from '../api.js'
import { t, lang, setLang } from '../i18n.js'
import { ask } from '../runner.js'
import { state, setTheme, loadIntegrations } from '../store.js'
import { isAdmin } from '../sections.js'
import Brand from '../components/Brand.vue'

const tokens = reactive({ github: '', cloudflare: '', resend: '' })
const INT = [
  { kind: 'github', name: 'GitHub', hint: 'int.github_hint', who: (i) => i.login && `@${i.login}` },
  { kind: 'cloudflare', name: 'Cloudflare', hint: 'int.cloudflare_hint', who: (i) => i.account },
  { kind: 'resend', name: 'Resend', hint: 'int.resend_hint', who: (i) => `${i.domains} domains` },
]
const connect = (k) => tokens[k].trim() && ask('set_integration', { kind: k, token: tokens[k].trim() }, { title: `${t('int.connect')}: ${k}`, after: () => { tokens[k] = ''; return loadIntegrations() } })
const disconnect = (k) => ask('remove_integration', { kind: k }, { title: `${t('int.disconnect')}: ${k}`, risk: 'destructive', after: loadIntegrations })
const origin = location.origin
</script>

<template>
  <div class="stack" style="max-width:880px">
    <div class="head"><h1>{{ t('nav.settings') }}</h1></div>
    <div class="card">
      <h3>{{ t('settings.appearance') }}</h3>
      <div class="frow"><span>{{ t('settings.theme') }}</span><div class="seg"><button v-for="v in ['system', 'light', 'dark']" :key="v" :class="{ on: state.theme === v }" @click="setTheme(v)">{{ t(`settings.${v}`) }}</button></div></div>
      <div class="frow"><span>{{ t('settings.language') }}</span><div class="seg"><button v-for="[v, l] in [['en', 'English'], ['ru', 'Русский'], ['az', 'Azərbaycanca']]" :key="v" :class="{ on: lang === v }" @click="setLang(v)">{{ l }}</button></div></div>
    </div>

    <div class="card" style="gap:16px">
      <h3>{{ t('nav.integrations') }}</h3>
      <div v-for="i in INT" :key="i.kind" class="stack" style="gap:6px">
        <div class="row"><Brand :name="i.kind" :size="16" /><b>{{ i.name }}</b>
          <span :class="['pill', state.integrations[i.kind]?.connected ? 'ok' : '']">{{ state.integrations[i.kind]?.connected ? t('int.connected_as', { who: i.who(state.integrations[i.kind]) || '' }) : t('int.not_connected') }}</span>
        </div>
        <div class="faint xs">{{ t(i.hint) }}</div>
        <div v-if="state.integrations[i.kind]?.error" class="error xs">{{ state.integrations[i.kind].error }}</div>
        <div v-if="isAdmin(session.user)" class="row">
          <input v-model="tokens[i.kind]" type="password" autocomplete="off" :placeholder="t('int.token')" style="max-width:380px">
          <button class="btn primary" @click="connect(i.kind)">{{ t('int.connect') }}</button>
          <button v-if="state.integrations[i.kind]" class="btn ghost" @click="disconnect(i.kind)">{{ t('int.disconnect') }}</button>
        </div>
      </div>
    </div>

    <div class="card">
      <h3>{{ t('agents.title') }}</h3>
      <div class="dim">{{ t('web.agents_lead') }}</div>
      <div class="kv"><span>{{ t('web.mcp_url') }}</span><span class="mono">POST {{ origin }}/api/tools/&lt;name&gt; · Authorization: Bearer &lt;token&gt;</span></div>
      <div v-if="isAdmin(session.user)"><router-link to="/team" class="btn">{{ t('web.add_agent') }}</router-link></div>
    </div>

    <div class="card">
      <h3>{{ t('settings.about') }}</h3>
      <div class="kv"><span>{{ t('settings.version') }}</span><span class="mono">Sesame Cloud 0.1.0-alpha.1</span></div>
      <div class="kv"><span>{{ t('settings.license') }}</span><span>Apache-2.0 · <a class="link" href="https://github.com/rasul-chopurov/sesame" target="_blank" rel="noopener">GitHub</a></span></div>
    </div>
  </div>
</template>
