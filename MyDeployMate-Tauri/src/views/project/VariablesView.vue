<script setup lang="ts">
import { Plus, RotateCcw, Trash2 } from '@lucide/vue'
import { computed, nextTick, ref, watch } from 'vue'

import DraftNotices from '@/components/DraftNotices.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import VariableValueInput from '@/components/variables/VariableValueInput.vue'
import {
  blankValue,
  clearOverride,
  describeSource,
  findOverride,
  removeVariable,
  renameVariable,
  setOverride,
} from '@/components/variables/variables'
import { useProjectDraft } from '@/composables/projectDraft'
import { call, type Pipeline, type Project, type Variable, type VariableView } from '@/ipc'
import { useSessionStore } from '@/stores/session'
import { useWorkspaceStore } from '@/stores/workspace'

// P04 variables and environment overrides (IXD-13). Project variables are edited here; global
// ones are shown read-only and can be overridden per environment. Effective values follow the
// draft; runs use the saved snapshot.
const props = defineProps<{ project: Project; pipeline: Pipeline | null }>()

const store = useWorkspaceStore()
const session = useSessionStore()
const page = ref<HTMLElement | null>(null)
const { draft, project, fieldError, outside, error } = useProjectDraft(
  '变量与环境',
  page,
  () => props.project.id,
)
const elsewhere = outside(['variables', 'overrides'])

const environmentId = computed(
  () => session.environment[props.project.id] ?? props.project.defaultEnvironmentId,
)
const environmentName = computed(() => store.environmentName(environmentId.value))

const globals = computed(() => draft.value?.globalVariables ?? [])
const inherited = computed(() => {
  const own = new Set(project.value?.variables.map((v) => v.name))
  return globals.value.filter((g) => !own.has(g.name))
})

// ---- Effective values (resolved by Rust from the draft) ------------------------------------

const views = ref(new Map<string, VariableView>())
/** Bumped when a Credential Manager value changes, so effective values are read again. */
const secretsChanged = ref(0)
let ticket = 0
let timer: ReturnType<typeof setTimeout> | undefined
watch(
  [draft, environmentId, secretsChanged],
  () => {
    clearTimeout(timer)
    timer = setTimeout(async () => {
      if (!draft.value) return
      const mine = ++ticket
      const list = await call('variable_views', {
        draft: draft.value,
        projectId: props.project.id,
        environmentId: environmentId.value,
      })
      if (mine === ticket) views.value = new Map(list.map((v) => [v.name, v]))
    }, 150)
  },
  { deep: true, immediate: true },
)

function sourceOf(variable: Variable) {
  const view = views.value.get(variable.name)
  if (!view || !project.value) return ''
  const value =
    view.origin === 'environmentOverride'
      ? findOverride(project.value, environmentId.value, variable.name)?.value
      : variable.value
  return describeSource(view.origin, value, environmentName.value)
}

// ---- Editing ------------------------------------------------------------------------------

function rename(variable: Variable, name: string) {
  if (project.value) renameVariable(project.value, globals.value, variable.name, name)
  variable.name = name
}

async function addVariable() {
  if (!project.value) return
  project.value.variables.push(await call('new_variable'))
  await nextTick()
  const inputs = page.value?.querySelectorAll<HTMLInputElement>('[data-var-name] input')
  inputs?.[inputs.length - 1]?.focus()
}

function remove(variable: Variable) {
  if (project.value) removeVariable(project.value, globals.value, variable.id)
}

function overrideOf(variable: Variable) {
  return project.value ? findOverride(project.value, environmentId.value, variable.name) : undefined
}

function addOverride(variable: Variable) {
  if (project.value)
    setOverride(project.value, environmentId.value, variable.name, blankValue(variable.sensitive))
}

function resetOverride(variable: Variable) {
  if (project.value) clearOverride(project.value, environmentId.value, variable.name)
}

/** Issue at a project override of `name` in the current environment. */
function overrideError(name: string): string | null {
  const i =
    project.value?.overrides.findIndex(
      (o) => o.environmentId === environmentId.value && o.name === name,
    ) ?? -1
  if (i < 0) return null
  return (
    fieldError(`overrides[${i}].value`) ??
    fieldError(`overrides[${i}].value.name`) ??
    fieldError(`overrides[${i}].name`)
  )
}

function variableError(i: number, field: 'name' | 'value'): string | null {
  return field === 'name'
    ? fieldError(`variables[${i}].name`)
    : (fieldError(`variables[${i}].value`) ?? fieldError(`variables[${i}].value.name`))
}

// Marking sensitive moves plain values out of the configuration (ARC-10); unmarking asks first,
// because the value may then be written back as plain text (IXD-13).
const unmarking = ref<Variable | null>(null)
function setSensitive(variable: Variable, sensitive: boolean) {
  if (!sensitive) {
    unmarking.value = variable
    return
  }
  variable.sensitive = true
  if (variable.value.kind === 'literal') variable.value = blankValue(true)
  for (const o of project.value?.overrides ?? []) {
    if (o.name === variable.name && o.value.kind === 'literal') o.value = blankValue(true)
  }
}
function confirmUnmark() {
  if (unmarking.value) unmarking.value.sensitive = false
  unmarking.value = null
}
</script>

<template>
  <div v-if="project" ref="page" class="variables">
    <DraftNotices :error="error" :issues="elsewhere" />

    <header class="head">
      <p class="hint">
        当前环境 <strong>{{ environmentName }}</strong>
        。覆盖值只作用于当前环境；“未设置”继承默认值，空文本也是一个已设置的值。
      </p>
      <UiButton :icon="Plus" compact @click="addVariable">新增变量</UiButton>
    </header>

    <div class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th scope="col" class="col-name">变量名</th>
            <th scope="col" class="col-value">默认值</th>
            <th scope="col" class="col-value">{{ environmentName }} 覆盖</th>
            <th scope="col" class="col-effective">生效值</th>
            <th scope="col" class="col-sensitive">敏感</th>
            <th scope="col" class="col-actions"><span class="sr-only">操作</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(v, i) in project.variables" :key="v.id">
            <td data-var-name class="col-name">
              <UiTextInput
                :model-value="v.name"
                mono
                :invalid="!!variableError(i, 'name')"
                :aria-label="`变量 ${i + 1} 名称`"
                @update:model-value="rename(v, $event)"
              />
              <p v-if="variableError(i, 'name')" class="cell-error">
                {{ variableError(i, 'name') }}
              </p>
            </td>
            <td class="col-value">
              <VariableValueInput
                v-model="v.value"
                :label="`${v.name || '变量'} 的默认值`"
                :sensitive="v.sensitive"
                :error="variableError(i, 'value')"
                :variable-id="v.id"
                @secret-saved="secretsChanged++"
              />
            </td>
            <td class="col-value">
              <template v-if="overrideOf(v)">
                <div class="override">
                  <VariableValueInput
                    v-model="overrideOf(v)!.value"
                    :label="`${v.name || '变量'} 在 ${environmentName} 的覆盖值`"
                    :sensitive="v.sensitive"
                    :error="overrideError(v.name)"
                    :variable-id="v.id"
                    :environment-id="environmentId"
                    @secret-saved="secretsChanged++"
                  />
                  <UiIconButton
                    :icon="RotateCcw"
                    :label="`删除 ${v.name} 的 ${environmentName} 覆盖，恢复继承`"
                    @click="resetOverride(v)"
                  />
                </div>
              </template>
              <span v-else class="unset">
                <span class="muted">未设置</span>
                <UiButton
                  compact
                  variant="ghost"
                  :disabled-reason="v.name ? null : '先填写变量名'"
                  @click="addOverride(v)"
                >
                  设置覆盖
                </UiButton>
              </span>
            </td>
            <td class="col-effective">
              <template v-if="views.get(v.name)">
                <code v-if="views.get(v.name)!.display" class="effective">{{
                  views.get(v.name)!.display
                }}</code>
                <span v-else-if="views.get(v.name)!.display === ''" class="effective muted"
                  >（空文本）</span
                >
                <span v-else class="cell-error">{{ views.get(v.name)!.error }}</span>
                <span class="source">{{ sourceOf(v) }}</span>
              </template>
            </td>
            <td class="col-sensitive">
              <UiCheckbox
                :model-value="v.sensitive"
                :label="`${v.name || '变量'} 为敏感变量`"
                hide-label
                @update:model-value="setSensitive(v, $event)"
              />
            </td>
            <td class="col-actions">
              <UiIconButton :icon="Trash2" :label="`删除变量 ${v.name}`" @click="remove(v)" />
            </td>
          </tr>

          <tr v-for="g in inherited" :key="g.id" class="inherited">
            <td class="col-name">
              <span class="global-name">
                <code>{{ g.name }}</code>
                <UiTag>全局</UiTag>
              </span>
            </td>
            <td class="col-value muted">在配置中心修改</td>
            <td class="col-value">
              <template v-if="overrideOf(g)">
                <div class="override">
                  <VariableValueInput
                    v-model="overrideOf(g)!.value"
                    :label="`${g.name} 在 ${environmentName} 的覆盖值`"
                    :sensitive="g.sensitive"
                    :error="overrideError(g.name)"
                    :variable-id="g.id"
                    :environment-id="environmentId"
                    @secret-saved="secretsChanged++"
                  />
                  <UiIconButton
                    :icon="RotateCcw"
                    :label="`删除 ${g.name} 的 ${environmentName} 覆盖，恢复继承`"
                    @click="resetOverride(g)"
                  />
                </div>
              </template>
              <span v-else class="unset">
                <span class="muted">未设置</span>
                <UiButton compact variant="ghost" @click="addOverride(g)">设置覆盖</UiButton>
              </span>
            </td>
            <td class="col-effective">
              <template v-if="views.get(g.name)">
                <code v-if="views.get(g.name)!.display" class="effective">{{
                  views.get(g.name)!.display
                }}</code>
                <span v-else-if="views.get(g.name)!.display === ''" class="effective muted"
                  >（空文本）</span
                >
                <span v-else class="cell-error">{{ views.get(g.name)!.error }}</span>
                <span class="source">{{ sourceOf(g) }}</span>
              </template>
            </td>
            <td class="col-sensitive">
              <UiTag v-if="g.sensitive" tone="warning">敏感</UiTag>
            </td>
            <td class="col-actions" />
          </tr>

          <tr v-if="!project.variables.length && !inherited.length">
            <td colspan="6" class="muted">尚无变量</td>
          </tr>
        </tbody>
      </table>
    </div>

    <UiConfirm
      :open="!!unmarking"
      title="取消敏感标记"
      confirm-label="取消敏感标记"
      @cancel="unmarking = null"
      @confirm="confirmUnmark"
    >
      <p>
        取消后，{{ unmarking?.name }}
        的值可以直接写进配置文件，日志和预览也不再遮蔽它。保存配置后生效。
      </p>
    </UiConfirm>
  </div>
</template>

<style scoped>
.variables {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.hint {
  flex: 1;
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.table-wrap {
  overflow-x: auto;
}

.table {
  min-width: 1080px;
}

.table td {
  height: auto;
  padding-top: var(--space-1);
  padding-bottom: var(--space-1);
  vertical-align: top;
}

.col-name {
  width: 200px;
}

.col-value {
  width: 300px;
}

.col-effective {
  min-width: 160px;
}

.col-sensitive {
  width: 56px;
  text-align: center;
}

.col-sensitive :deep(.checkbox) {
  height: var(--input-height);
}

.col-actions {
  width: 48px;
}

.override {
  display: flex;
  align-items: flex-start;
  gap: var(--space-1);
}

.override > :first-child {
  flex: 1;
  min-width: 0;
}

.unset {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  height: var(--input-height);
}

.global-name {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  height: var(--input-height);
}

.inherited .col-value.muted {
  line-height: var(--input-height);
}

.effective {
  display: block;
  padding-top: 8px;
  word-break: break-all;
  user-select: text;
}

.source {
  display: block;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.cell-error {
  display: block;
  margin: var(--space-1) 0 0;
  color: var(--state-failure);
  font: var(--font-caption);
}

.muted {
  color: var(--text-secondary);
}
</style>
