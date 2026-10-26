// Keeps the app shell available offline; API calls always go to the network.
const CACHE = 'sesame-shell-v1'
self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(['/', '/manifest.webmanifest', '/logo.svg'])))
  self.skipWaiting()
})
self.addEventListener('activate', (e) => {
  e.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))))
  self.clients.claim()
})
self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url)
  if (e.request.method !== 'GET' || url.pathname.startsWith('/api/')) return
  e.respondWith(
    fetch(e.request)
      .then((r) => {
        const copy = r.clone()
        caches.open(CACHE).then((c) => c.put(e.request, copy))
        return r
      })
      .catch(() => caches.match(e.request).then((r) => r || caches.match('/'))),
  )
})
