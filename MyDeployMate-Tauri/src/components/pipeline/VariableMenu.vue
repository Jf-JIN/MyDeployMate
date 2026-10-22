<script setup lang="ts">
import { Braces } from '@lucide/vue'
import { computed, nextTick } from 'vue'

import UiMenu from '@/components/ui/UiMenu.vue'
import type { MenuEntry } from '@/components/ui/types'

import { insertExpression } from './pipeline'
import { useVariableMenu } from './variableMenu'

// IXD-11: insert a template expression at the cursor of the field `fieldId`. Variables are
// grouped by origin; sensitive ones show only their name. Server built-ins are not offered
// where there is no target server (local commands).
const model = defineModel<string>({ required: true })
const props = defineProps<{ fieldId: string; local?: boolean; fieldLabel: string }>()

const source = useVariableMenu()

const ORIGIN = { global: '全局', project: '项目', environmentOverride: '环境覆盖', run: '运行时' }

const entries = computed<MenuEntry[]>(() => {
  if (!source) return []
  const vars = source.variables.value
  const out: MenuEntry[] = [
    { key: 'h-vars', label: `变量（${source.environmentName.value}）`, kind: 'heading' },
  ]
  if (!vars.length)
    out.push({ key: 'none', label: '尚无变量', disabledReason: '在“变量与环境”中添加' })
  for (const v of vars) {
    out.push({
      key: `\${${v.name}}`,
      label: v.name,
      detail: v.sensitive ? `${ORIGIN[v.origin]} · 敏感` : ORIGIN[v.origin],
    })
  }
  out.push({ key: 'h-builtin', label: '内建变量', kind: 'heading' })
  for (const b of source.builtins.value) {
    out.push({
      key: b.expression,
      label: b.expression,
      detail: b.label,
      disabledReason: props.local && b.needsServer ? '本地命令没有目标服务器' : null,
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
    :label="`在${fieldLabel}中插入变量`"
    :entries="entries"
    :icon="Braces"
    icon-only
    align="end"
    @select="insert"
  />
</template>
