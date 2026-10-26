<script setup>
import { watchEffect } from 'vue'
import { useRouter } from 'vue-router'
import { session } from '../api.js'
import { t } from '../i18n.js'
import { state } from '../store.js'
import { isAdmin } from '../sections.js'

// Like the desktop, open the first server; with none, offer to add one.
const router = useRouter()
watchEffect(() => {
  const first = state.servers[0]
  if (first) router.replace(`/servers/${encodeURIComponent(first.name)}/overview`)
})
</script>

<template>
  <div class="empty">
    <img src="/logo.svg" width="56" height="56" alt="" style="opacity:.9"><p>{{ t('app.tagline') }}</p>
    <p class="dim">{{ t('web.no_servers') }}</p>
    <router-link v-if="isAdmin(session.user)" to="/add-server" class="btn primary">{{ t('nav.add_server') }}</router-link>
  </div>
</template>
