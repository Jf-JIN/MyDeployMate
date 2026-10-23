import { config } from '@vue/test-utils'

import { i18n } from '@/i18n'

// Components render their text through vue-i18n (ARC-38); mounted components need the plugin.
config.global.plugins.push(i18n)
