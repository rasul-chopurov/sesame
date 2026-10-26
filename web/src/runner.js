// One dialog for every change: shows what will happen, asks for the name on
// destructive steps, runs the tool and shows the result.
import { reactive } from 'vue'
import { tool, session } from './api.js'

export const run = reactive({ open: false, title: '', details: [], risk: 'change', word: '', typed: '', status: 'ask', output: '', error: '', name: '', args: {}, after: null })

export function ask(name, args, { title, details = [], risk = 'change', word = '', after = null } = {}) {
  Object.assign(run, { open: true, name, args, title: title || name, details, risk, word, typed: '', status: 'ask', output: '', error: '', after })
}

export async function confirm() {
  run.status = needsApproval() ? 'approval' : 'running'
  try {
    const r = await tool(run.name, run.args)
    run.output = r.output || JSON.stringify(r, null, 2)
    run.status = 'done'
    if (run.after) run.after()
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
