<script setup>
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { session } from '../api.js'
import { t } from '../i18n.js'
import { state, rediscover } from '../store.js'
import { ask } from '../runner.js'
import { SECTIONS, isAdmin, short } from '../sections.js'
import Icon from '../components/Icon.vue'
import Overview from './sections/Overview.vue'
import Sites from './sections/Sites.vue'
import Processes from './sections/Processes.vue'
import Services from './sections/Services.vue'
import Databases from './sections/Databases.vue'
import MailCron from './sections/MailCron.vue'
import Security from './sections/Security.vue'
import Terminal from './sections/Terminal.vue'
import NewSite from './sections/NewSite.vue'

const props = defineProps({ name: String, section: String })
const router = useRouter()
const server = computed(() => state.servers.find((s) => s.name === props.name))
const snap = computed(() => state.snaps[props.name])
const views = { overview: Overview, sites: Sites, processes: Processes, services: Services, databases: Databases, 'mail-cron': MailCron, security: Security, terminal: Terminal, 'new-site': NewSite }
const title = computed(() => (props.section === 'new-site' ? t('ns.title') : t(SECTIONS.find((s) => s.slug === props.section)?.key || 'nav.overview')))

function forget() {
  ask('remove_server', { server: props.name }, {
    title: t('server.forget'),
    details: [t('server.forget_lead', { name: props.name })],
    risk: 'destructive',
    word: props.name,
    after: async () => { state.servers = state.servers.filter((s) => s.name !== props.name); router.push('/') },
  })
}
</script>

<template>
  <div class="stack">
    <div class="head">
      <div>
        <h1>{{ title }}</h1>
        <div class="dim">{{ name }}<template v-if="snap"> · {{ snap.os.distro }}</template></div>
      </div>
      <div class="row">
        <button class="btn primary" :disabled="state.loading[name]" @click="rediscover(name)">
          <Icon name="refresh" :size="14" />{{ state.loading[name] ? t('server.discovering') : snap ? t('server.rediscover') : t('server.discover') }}
        </button>
        <button v-if="isAdmin(session.user)" class="btn ghost" @click="forget">{{ t('server.forget') }}</button>
      </div>
    </div>
    <div v-if="state.errors[name]" class="card">
      <h3>{{ t('server.could_not_discover') }}</h3>
      <div class="error">{{ state.errors[name] }}</div>
    </div>
    <div v-if="!server" class="empty">{{ t('server.empty') }}</div>
    <div v-else-if="!snap" class="empty">{{ state.loading[name] ? t('server.discovering') : t('server.not_discovered') }}</div>
    <component :is="views[section] || Overview" v-else :name="name" :snap="snap" />
    <div v-if="snap" class="faint xs">{{ t('server.discovered_at', { time: short(snap.taken_at) }) }}</div>
  </div>
</template>
