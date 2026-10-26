<script setup>
import { session } from '../../api.js'
import { t } from '../../i18n.js'
import { ask } from '../../runner.js'
import { rediscover } from '../../store.js'
import { canChange } from '../../sections.js'
import Icon from '../../components/Icon.vue'

const props = defineProps({ name: String, snap: Object })
const color = (s) => (['RUNNING', 'running'].includes(s) ? 'ok' : ['STARTING', 'BACKOFF', 'exited'].includes(s) ? 'warn' : ['FATAL', 'failed'].includes(s) ? 'bad' : '')
const act = (kind, unit, action) => ask('service_action', { server: props.name, kind, name: unit, action }, { title: `${t(`proc.${action}`)} ${unit}`, after: () => rediscover(props.name) })
const actions = (kind, running) => (running ? ['restart', 'stop', ...(kind === 'systemd' ? ['disable'] : [])] : ['start'])
const icon = { restart: 'refresh', stop: 'square', disable: 'power', start: 'play' }
</script>

<template>
  <div class="stack">
    <div class="card">
      <h3>{{ t('proc.supervisor') }}</h3>
      <div v-if="!snap.supervisor.length" class="dim">{{ t('proc.none') }}</div>
      <div v-for="p in snap.supervisor" :key="p.name" class="row wrap" style="min-height:30px">
        <span class="dot" :style="{ background: `var(--${color(p.state) || 'faint'})` }" />
        <span :class="['pill', color(p.state)]" style="width:84px;justify-content:center">{{ p.state }}</span>
        <span class="b grow">{{ p.name }}</span>
        <span class="dim" style="width:80px">{{ p.role }}</span><span class="dim mono xs" style="width:170px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ p.user }}</span>
        <span class="faint" style="width:150px">{{ p.uptime ? t('proc.up', { time: p.uptime }) : '' }}</span>
        <span v-if="canChange(session.user)" class="row" style="gap:2px">
          <button v-for="a in actions('supervisor', p.state === 'RUNNING')" :key="a" class="btn ghost xs" :title="t(`proc.${a}`)" @click="act('supervisor', p.name, a)"><Icon :name="icon[a]" :size="13" /></button>
        </span>
      </div>
    </div>
    <div class="card">
      <h3>{{ t('proc.system') }}</h3>
      <div v-for="s in snap.services.filter((x) => x.active !== 'inactive')" :key="s.unit" class="row wrap" style="min-height:28px">
        <span class="dot" :style="{ background: `var(--${color(s.sub) || 'faint'})` }" />
        <span :class="['pill', color(s.sub)]" style="width:84px;justify-content:center">{{ s.sub }}</span>
        <span style="width:240px" class="mono">{{ s.unit.replace(/\.service$/, '') }}</span>
        <span class="dim grow" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ s.description }}</span>
        <span v-if="canChange(session.user)" class="row" style="gap:2px">
          <button v-for="a in actions('systemd', s.sub === 'running')" :key="a" class="btn ghost xs" :title="t(`proc.${a}`)" @click="act('systemd', s.unit, a)"><Icon :name="icon[a]" :size="13" /></button>
        </span>
      </div>
    </div>
  </div>
</template>
