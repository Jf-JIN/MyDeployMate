import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript'
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting'
import pluginVue from 'eslint-plugin-vue'

export default defineConfigWithVueTs(
  {
    ignores: [
      'dist/**',
      'node_modules/**',
      'target/**',
      'src-tauri/**',
      'crates/**',
      'spikes/**',
      'demo/**',
      'test-env/**',
      'src/ipc/bindings/**',
    ],
  },
  pluginVue.configs['flat/recommended'],
  vueTsConfigs.recommended,
  {
    rules: {
      // Logs and user input are rendered as text only (OBS-01).
      'vue/no-v-html': 'error',
    },
  },
  skipFormatting,
)
