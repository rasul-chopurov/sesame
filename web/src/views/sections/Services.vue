<script setup>
import { computed, reactive } from 'vue'
import { useRouter } from 'vue-router'
import { session } from '../../api.js'
import { t } from '../../i18n.js'
import { ask } from '../../runner.js'
import { state, rediscover } from '../../store.js'
import { canChange } from '../../sections.js'
import Icon from '../../components/Icon.vue'
import Brand from '../../components/Brand.vue'

const props = defineProps({ name: String, snap: Object })
const router = useRouter()
const DESC = { redis: 'svc.redis', postgresql: 'svc.postgres', mariadb: 'svc.mariadb', mongodb: 'svc.mongodb', elasticsearch: 'svc.elasticsearch', rustfs: 'svc.s3', stalwart: 'svc.stalwart', phpmyadmin: 'svc.phpmyadmin', pgadmin: 'svc.pgadmin' }
const LOGO = { rustfs: 'minio', postgresql: 'postgresql', phpmyadmin: 'mysql', pgadmin: 'postgresql', stalwart: '' }
const PANELS = ['phpmyadmin', 'pgadmin']
const list = computed(() => state.services[props.name] || [])
const catalog = computed(() => list.value.filter((s) => !PANELS.includes(s.service)))
const panels = computed(() => list.value.filter((s) => PANELS.includes(s.service)))
const wipe = reactive({})
const can = computed(() => canChange(session.user))
const reverbs = computed(() => props.snap.supervisor.filter((p) => p.role === 'reverb').length)
const has = (...kinds) => props.snap.databases.some((d) => kinds.includes(d.kind))
const panelOk = (p) => (p === 'phpmyadmin' ? has('my_sql', 'maria_db') : has('postgres'))
const label = (s) => (s.service === 'rustfs' ? 'S3 · RustFS' : s.name)
const refresh = () => rediscover(props.name)

const install = (s) => ask('install_service', { server: props.name, service: s.service }, { title: `${t('svc.install')} ${label(s)}`, after: refresh })
const uninstall = (s) => ask('uninstall_service', { server: props.name, service: s.service, delete_data: !!wipe[s.service] }, {
  title: `${t('svc.uninstall')} ${label(s)}`, risk: 'destructive', word: s.service,
  details: wipe[s.service] ? [t('job.delete_data')] : [], after: refresh,
})
</script>

<template>
  <div class="stack">
    <div><div style="font-size:16px;font-weight:600">{{ t('svc.title') }}</div><div class="dim">{{ t('svc.lead') }}</div></div>
    <div class="grid2">
      <div v-for="s in catalog" :key="s.service" class="card svc">
        <div class="row">
          <div class="logo"><Brand v-if="LOGO[s.service] !== ''" :name="LOGO[s.service] || s.service" :size="20" /><Icon v-else name="mail" :size="18" style="color:var(--accent)" /></div>
          <div class="grow"><div class="b">{{ label(s) }}</div><div class="dim xs">{{ t(DESC[s.service]) }}</div></div>
        </div>
        <div class="row between wrap">
          <span v-if="s.installed" class="pill ok">{{ t('svc.installed') }}{{ s.version ? ` · ${s.version}` : '' }}</span>
          <span v-else-if="s.conflict" class="pill warn">{{ t('svc.conflict', { other: s.conflict }) }}</span>
          <span v-else class="pill">{{ t('svc.not_installed') }}</span>
          <span v-if="can" class="row">
            <template v-if="s.installed">
              <label class="row xs dim" style="gap:4px"><input v-model="wipe[s.service]" type="checkbox" style="width:auto">{{ t('job.delete_data') }}</label>
              <button class="btn ghost" @click="uninstall(s)">{{ t('svc.uninstall') }}</button>
            </template>
            <button v-else class="btn primary" :disabled="!!s.conflict" @click="install(s)"><Icon name="download" :size="14" />{{ t('svc.install') }}</button>
          </span>
        </div>
      </div>
      <div class="card svc">
        <div class="row"><div class="logo"><Brand name="laravel" :size="20" /></div><div class="grow"><div class="b">Reverb</div><div class="dim xs">{{ t('svc.reverb') }}</div></div></div>
        <div class="row between">
          <span :class="['pill', reverbs ? 'ok' : '']">{{ reverbs ? `${t('svc.installed')} · ${reverbs}` : t('svc.not_installed') }}</span>
          <button class="btn ghost" @click="router.push(`/servers/${encodeURIComponent(name)}/sites`)"><Icon name="globe" :size="14" />{{ t('svc.per_site') }}</button>
        </div>
      </div>
    </div>

    <div class="card" style="padding:0;border:0;background:none;backdrop-filter:none"><h3>{{ t('svc.panels') }}</h3></div>
    <div class="grid2">
      <div v-for="p in panels" :key="p.service" class="card svc">
        <div class="row"><div class="logo"><Brand :name="LOGO[p.service]" :size="20" /></div><div class="grow"><div class="b">{{ p.name }}</div><div class="dim xs">{{ t(DESC[p.service]) }}</div></div></div>
        <div class="row between wrap">
          <span v-if="p.installed" class="pill ok">{{ t('svc.installed') }} · 127.0.0.1:{{ p.local_port }}</span>
          <span v-else-if="!panelOk(p.service)" class="pill">{{ t('svc.needs', { what: p.service === 'phpmyadmin' ? 'MySQL / MariaDB' : 'PostgreSQL' }) }}</span>
          <span v-else class="pill">{{ t('svc.not_installed') }}</span>
          <span v-if="can" class="row">
            <button v-if="p.installed" class="btn ghost" @click="uninstall(p)">{{ t('svc.uninstall') }}</button>
            <button v-else class="btn primary" :disabled="!panelOk(p.service)" @click="install(p)"><Icon name="download" :size="14" />{{ t('svc.install') }}</button>
          </span>
        </div>
        <div v-if="p.installed" class="faint xs">{{ t('web.panel_desktop') }}</div>
      </div>
    </div>
    <div class="faint xs">{{ t('svc.note') }}</div>
  </div>
</template>

<style>
.svc { gap: 12px; }
.logo { width: 36px; height: 36px; border-radius: 9px; background: var(--hover); display: grid; place-items: center; flex: none; }
</style>
