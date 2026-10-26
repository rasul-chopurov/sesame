import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import Login from './views/Login.vue'
import Servers from './views/Servers.vue'
import Server from './views/Server.vue'
import Approvals from './views/Approvals.vue'
import Audit from './views/Audit.vue'
import Team from './views/Team.vue'
import { session } from './api.js'
import './style.css'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/login', component: Login, meta: { public: true } },
    { path: '/', component: Servers },
    { path: '/servers/:name', component: Server, props: true },
    { path: '/approvals', component: Approvals },
    { path: '/audit', component: Audit },
    { path: '/team', component: Team },
  ],
})

router.beforeEach((to) => {
  if (!to.meta.public && !session.token) return '/login'
})

createApp(App).use(router).mount('#app')

if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {})
