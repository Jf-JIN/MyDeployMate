<script setup lang="ts">
import { Braces } from '@lucide/vue'
import { computed, nextTick } from 'vue'

import UiMenu from '@/components/ui/UiMenu.vue'
import type { MenuEntry } from '@/components/ui/types'
import { useT } from '@/i18n'

import { insertExpression } from './pipeline'
import { useVariableMenu } from './variableMenu'

const t = useT()

// IXD-11: insert a template expression at the cursor of the field `fieldId`. Variables are
// grouped by origin; sensitive ones show only their name. Server built-ins are not offered
// where there is no target server (local commands).
const model = defineModel<string>({ required: true })
const props = defineProps<{ fieldId: string; local?: boolean; fieldLabel: string }>()

const source = useVariableMenu()

const entries = computed<MenuEntry[]>(() => {
  if (!source) return []
  const vars = source.variables.value
  const out: MenuEntry[] = [
    {
      key: 'h-vars',
      label: t('variableMenu.heading', { environment: source.environmentName.value }),
      kind: 'heading',
    },
  ]
  if (!vars.length)
    out.push({
      key: 'none',
      label: t('variableMenu.none'),
      disabledReason: t('variableMenu.addHint'),
    })
  for (const v of vars) {
    const origin = t(`variableMenu.origin.${v.origin}`)
    out.push({
      key: `\${${v.name}}`,
      label: v.name,
      detail: v.sensitive ? t('variableMenu.sensitive', { origin }) : origin,
    })
  }
  out.push({ key: 'h-builtin', label: t('variableMenu.builtin'), kind: 'heading' })
  for (const b of source.builtins.value) {
    out.push({
      key: b.expression,
      label: b.expression,
      detail: b.label,
      disabledReason: props.local && b.needsServer ? t('variableMenu.noServer') : null,
    })
  }
  return out
})

async function insert(expression: string) {
  const el = document.getElementById(props.fieldId) as HTMLInputElement | HTMLTextAreaElement | null
  const value = model.value
  const start = el?.selectionStart ?? value.length
  const end = el?.selectionEnd ?? start
  const next = insertExpression(value, start, end, expression)
  model.value = next.value
  await nextTick()
  el?.focus()
  el?.setSelectionRange(next.caret, next.caret)
}
</script>

<template>
  <UiMenu
    :label="t('variableMenu.insert', { field: fieldLabel })"
    :entries="entries"
    :icon="Braces"
    icon-only
    tonal
    align="end"
    @select="insert"
  />
</template>
