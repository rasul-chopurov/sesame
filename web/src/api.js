// Tiny client for the sesame-cloud API. The session token lives in
// localStorage so the installed (PWA) app stays signed in.
import { reactive } from 'vue'

export const session = reactive({ token: read('sesame.token'), user: null })

function read(k) {
  try { return localStorage.getItem(k) } catch { return null }
}
function write(k, v) {
  try { v == null ? localStorage.removeItem(k) : localStorage.setItem(k, v) } catch { /* private mode */ }
}

export async function api(path, { method = 'GET', body } = {}) {
  const headers = { 'content-type': 'application/json' }
  if (session.token) headers.authorization = `Bearer ${session.token}`
  const r = await fetch(`/api${path}`, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) })
  const data = await r.json().catch(() => ({}))
  if (r.status === 401 && session.token) signOut(false)
  if (!r.ok) throw new Error(data.error || `HTTP ${r.status}`)
  return data
}

export function tool(name, args = {}) {
  return api(`/tools/${name}`, { method: 'POST', body: args })
}

export function signIn(token, user) {
  session.token = token
  session.user = user
  write('sesame.token', token)
}

export function signOut(callServer = true) {
  if (callServer && session.token) api('/logout', { method: 'POST' }).catch(() => {})
  session.token = null
  session.user = null
  write('sesame.token', null)
}
