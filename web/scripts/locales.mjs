// Turns the desktop app's locales (crates/sesame-app/locales/app.yml) into
// src/locales.json, so the web app speaks exactly like the desktop.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const yml = readFileSync(join(here, '../../crates/sesame-app/locales/app.yml'), 'utf8')
const out = { en: {}, ru: {}, az: {} }
for (const line of yml.split('\n')) {
  const m = line.match(/^([a-z0-9_.]+):\s*\{(.*)\}\s*$/)
  if (!m) continue
  for (const [, lang, value] of m[2].matchAll(/(en|ru|az):\s*"((?:[^"\\]|\\.)*)"/g)) {
    out[lang][m[1]] = JSON.parse(`"${value}"`)
  }
}
writeFileSync(join(here, '../src/locales.json'), JSON.stringify(out))
console.log(`locales: ${Object.keys(out.en).length} keys`)
