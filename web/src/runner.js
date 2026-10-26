// One dialog for every change, like the desktop's confirmation: what will
// happen, the name typed for destructive steps, then the live result.
import { reactive } from 'vue'
import { tool, session } from './api.js'

export const run = reactive({ open: false, title: '', details: [], risk: 'change', word: '', typed: '', status: 'ask', output: '', error: '', secrets: null, name: '', args: {}, after: null })

export function ask(name, args, { title, details = [], risk = 'change', word = '', after = null } = {}) {
  Object.assign(run, { open: true, name, args, title: title || name, details, risk, word, typed: '', status: 'ask', output: '', error: '', secrets: null, after })
}

/** Runs a change right away (for steps the user already confirmed in a form). */
export function runNow(name, args, opts = {}) {
  ask(name, args, opts)
  return confirm()
}

export async function confirm() {
  run.status = needsApproval() ? 'approval' : 'running'
  try {
    const r = await tool(run.name, run.args)
    run.output = r.output ?? (typeof r === 'string' ? r : '')
    run.secrets = r.secrets && typeof r.secrets === 'object' && Object.keys(r.secrets).length ? r.secrets : null
    run.status = 'done'
    if (run.after) await run.after(r)
  } catch (e) {
    run.error = e.message
    run.status = 'failed'
  }
}

export function needsApproval() {
  const role = session.user?.role
  return run.risk === 'destructive' && role !== 'admin' && role !== 'owner'
}

export function close() {
  run.open = false
}
