import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript'
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting'
import pluginVue from 'eslint-plugin-vue'
import vueI18n from '@intlify/eslint-plugin-vue-i18n'

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
  {
    // Interface text comes from src/i18n (NFR-05, ARC-38); the dev gallery is exempt.
    files: ['src/**/*.vue'],
    ignores: ['src/views/dev/**'],
    plugins: { '@intlify/vue-i18n': vueI18n },
    rules: {
      '@intlify/vue-i18n/no-raw-text': [
        'error',
        // Symbols, numbers and separators are not text to translate.
        { ignorePattern: '^[\\s\\d\\p{P}\\p{S}]*$' },
      ],
    },
  },
  skipFormatting,
)
