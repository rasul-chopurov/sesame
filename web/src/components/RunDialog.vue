<script setup>
import { t } from '../i18n.js'
import { run, confirm, close } from '../runner.js'

function copyEnv() {
  navigator.clipboard?.writeText(Object.entries(run.secrets).map(([k, v]) => `${k}=${v}`).join('\n'))
}
</script>

<template>
  <div v-if="run.open" class="scrim" @click.self="!['running', 'approval'].includes(run.status) && close()">
    <div class="dialog card">
      <div class="row between">
        <b>{{ run.status === 'ask' ? t('job.confirm_title') : run.title }}</b>
        <span :class="['pill', run.risk === 'destructive' ? 'bad' : 'warn']">{{ run.risk === 'destructive' ? t('job.risk_destructive') : t('job.risk_change') }}</span>
      </div>
      <template v-if="run.status === 'ask'">
        <div class="b">{{ run.title }}</div>
        <div v-if="run.details.length" class="dim">{{ t('job.will') }}</div>
        <ul v-if="run.details.length" class="dim" style="margin:0;padding-left:18px"><li v-for="d in run.details" :key="d">{{ d }}</li></ul>
        <div v-if="run.word" class="stack" style="gap:6px">
          <div class="dim">{{ t('job.type_to_confirm', { word: run.word }) }}</div>
          <input v-model="run.typed" autofocus autocomplete="off">
        </div>
        <div class="row" style="justify-content:flex-end">
          <button class="btn" @click="close">{{ t('job.cancel') }}</button>
          <button :class="['btn', run.risk === 'destructive' ? 'danger' : 'primary']" :disabled="!!run.word && run.typed.trim() !== run.word" @click="confirm">{{ t('job.run') }}</button>
        </div>
      </template>
      <p v-else-if="run.status === 'running'" class="dim">{{ t('job.running') }}</p>
      <p v-else-if="run.status === 'approval'" class="dim">{{ t('web.waiting_approval') }}</p>
      <template v-else>
        <span :class="['pill', run.status === 'done' ? 'ok' : 'bad']" style="justify-self:start">{{ run.status === 'done' ? t('job.done') : t('job.failed') }}</span>
        <p v-if="run.error" class="error" style="margin:0;white-space:pre-wrap">{{ run.error }}</p>
        <pre v-if="run.output" class="mono out">{{ run.output }}</pre>
        <template v-if="run.secrets">
          <div class="dim xs">{{ t('job.reveal') }}</div>
          <div v-for="(v, k) in run.secrets" :key="k" class="kv mono"><span>{{ k }}</span><span>{{ v }}</span></div>
          <button class="btn xs" style="justify-self:start" @click="copyEnv">{{ t('job.copy_env') }}</button>
        </template>
        <div class="row" style="justify-content:flex-end"><button class="btn" @click="close">{{ t('job.close') }}</button></div>
      </template>
    </div>
  </div>
</template>

<style>
.scrim { position: fixed; inset: 0; background: rgb(0 0 0 / .35); display: grid; place-items: center; padding: 16px; z-index: 50; }
.dialog { width: min(560px, 100%); background: var(--panel-solid); gap: 12px; padding: 18px; box-shadow: 0 30px 80px rgb(0 0 0 / .35); }
.out { max-height: 300px; overflow: auto; background: var(--hover); padding: 10px; border-radius: 8px; white-space: pre-wrap; margin: 0; }
</style>
