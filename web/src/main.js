import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import { session } from './api.js'
import { applyTheme } from './store.js'
import './style.css'

const v = (name) => () => import(`./views/${name}.vue`)

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/login', component: v('Login'), meta: { public: true } },
    { path: '/', component: v('Home') },
    { path: '/servers/:name', redirect: (to) => `/servers/${to.params.name}/overview` },
    { path: '/servers/:name/sites/:domain', component: v('Site'), props: true },
    { path: '/servers/:name/files/:domain', component: v('Files'), props: true },
    { path: '/servers/:name/:section', component: v('ServerPage'), props: true },
    { path: '/add-server', component: v('AddServer') },
    { path: '/cloudflare', component: v('Cloudflare') },
    { path: '/settings', component: v('Settings') },
    { path: '/approvals', component: v('Approvals') },
    { path: '/audit', component: v('Audit') },
    { path: '/team', component: v('Team') },
  ],
})

router.beforeEach((to) => {
  if (!to.meta.public && !session.token) return '/login'
})

applyTheme()
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme)
createApp(App).use(router).mount('#app')

if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {})
