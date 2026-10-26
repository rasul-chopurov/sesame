import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// `npm run dev` proxies the API to a local sesame-cloud.
export default defineConfig({
  plugins: [vue()],
  server: { proxy: { '/api': 'http://127.0.0.1:7880' } },
})
