<script setup lang="ts">
import { FolderOpen, Plus, Shield, Trash2 } from '@lucide/vue'
import { computed, nextTick, onMounted, ref } from 'vue'

import PageLayout from '@/components/PageLayout.vue'
import { describeReference } from '@/components/references'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useWorkspaceDraft } from '@/composables/workspaceDraft'
import {
  call,
  toAppError,
  type AppInfo,
  type Environment,
  type Reference,
  type Variable,
  type VariableValue,
} from '@/ipc'

// P10 configuration center, phase 3.2: environments, global variables and the data directory
// (IXD-14, IXD-36). Saved credentials, host trust, import/export and updates join in later
// phases.
const page = ref<HTMLElement | null>(null)
const { draft, dirty, at, issues, error, save, discard } = useWorkspaceDraft('配置中心', page)
const saving = ref(false)

const info = ref<AppInfo | null>(null)
const infoError = ref<string | null>(null)
onMounted(async () => {
  try {
    info.value = await call('app_info')
  } catch (e) {
    infoError.value = toAppError(e).message
  }
})

async function onSave() {
  saving.value = true
  try {
    await save()
  } finally {
    saving.value = false
  }
}

/** Issues outside this page that still block saving the workspace. */
const elsewhere = computed(() =>
  issues.value.filter(
    (i) => !i.path.startsWith('environments[') && !i.path.startsWith('globalVariables['),
  ),
)

// ---- Environments -------------------------------------------------------------------------

async function addEnvironment() {
  if (!draft.value) return
  draft.value.environments.push(await call('new_environment'))
  await nextTick()
  const inputs = page.value?.querySelectorAll<HTMLInputElement>('[data-env-name] input')
  inputs?.[inputs.length - 1]?.focus()
}

// IXD-14: changing protection is confirmed on its own.
const protectTarget = ref<Environment | null>(null)
function requestProtect(env: Environment) {
  protectTarget.value = env
}
function applyProtect() {
  if (protectTarget.value) protectTarget.value.protected = !protectTarget.value.protected
  protectTarget.value = null
}

// IXD-14: a referenced environment shows its references instead of being deleted.
const deleteTarget = ref<Environment | null>(null)
const blockingRefs = ref<Reference[]>([])
const blockedEnvironment = ref<Environment | null>(null)
async function requestDelete(env: Environment) {
  if (!draft.value) return
  const refs = await call('environment_references', { draft: draft.value, environmentId: env.id })
  if (refs.length) {
    blockingRefs.value = refs
    blockedEnvironment.value = env
  } else {
    deleteTarget.value = env
  }
}
function applyDelete() {
  if (draft.value && deleteTarget.value) {
    const id = deleteTarget.value.id
    draft.value.environments = draft.value.environments.filter((e) => e.id !== id)
  }
  deleteTarget.value = null
}

// ---- Global variables ---------------------------------------------------------------------

type SourceKind = VariableValue['kind']

function sourceOptions(variable: Variable) {
  return [
    {
      value: 'literal' as SourceKind,
      label: '直接填写',
      disabledReason: variable.sensitive ? '敏感变量不能以明文保存' : undefined,
    },
    { value: 'envRef' as SourceKind, label: '环境变量' },
    { value: 'secretRef' as SourceKind, label: '凭据管理器' },
  ]
}

function setSource(variable: Variable, kind: SourceKind) {
  if (variable.value.kind === kind) return
  variable.value =
    kind === 'literal'
      ? { kind, value: '' }
      : kind === 'envRef'
        ? { kind, name: '' }
        : { kind: 'secretRef' }
}

function setSensitive(variable: Variable, sensitive: boolean) {
  variable.sensitive = sensitive
  // A sensitive value never stays as plain text in the draft (ARC-10).
  if (sensitive && variable.value.kind === 'literal') variable.value = { kind: 'envRef', name: '' }
}

async function addVariable() {
  if (!draft.value) return
  draft.value.globalVariables.push(await call('new_variable'))
  await nextTick()
  const inputs = page.value?.querySelectorAll<HTMLInputElement>('[data-var-name] input')
  inputs?.[inputs.length - 1]?.focus()
}

function removeVariable(index: number) {
  draft.value?.globalVariables.splice(index, 1)
}

async function openDataDir() {
  try {
    await call('open_data_dir')
  } catch (e) {
    infoError.value = toAppError(e).message
  }
}
</script>

<template>
  <PageLayout title="配置中心">
    <template #actions>
      <UiTag v-if="dirty" tone="warning">未保存</UiTag>
      <UiButton :disabled-reason="dirty ? null : '没有未保存的更改'" @click="discard">
        放弃更改
      </UiButton>
      <UiButton
        variant="primary"
        :busy="saving"
        :disabled-reason="dirty ? null : '没有未保存的更改'"
        @click="onSave"
      >
        保存
      </UiButton>
    </template>

    <div v-if="draft" ref="page" class="settings">
      <UiNotice v-if="error" tone="failure">{{ error.message }}</UiNotice>
      <UiNotice v-if="elsewhere.length" tone="warning">
        配置的其他位置还有 {{ elsewhere.length }} 个问题，需要先修正才能保存：
        <ul class="elsewhere">
          <li v-for="issue in elsewhere" :key="issue.path">{{ issue.message }}</li>
        </ul>
      </UiNotice>

      <section class="section" aria-labelledby="env-title">
        <header class="section-head">
          <h2 id="env-title">环境</h2>
          <p class="section-hint">保护环境在执行前需要键入环境名确认；改名不影响已有绑定。</p>
          <UiButton :icon="Plus" compact @click="addEnvironment">新增环境</UiButton>
        </header>
        <table class="table">
          <thead>
            <tr>
              <th scope="col">名称</th>
              <th scope="col" class="col-protect">保护</th>
              <th scope="col" class="col-actions"><span class="sr-only">操作</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(env, i) in draft.environments" :key="env.id">
              <td data-env-name>
                <UiTextInput
                  v-model="env.name"
                  :invalid="!!at(`environments[${i}].name`)"
                  :aria-label="`环境 ${i + 1} 名称`"
                />
                <p v-if="at(`environments[${i}].name`)" class="cell-error">
                  {{ at(`environments[${i}].name`) }}
                </p>
              </td>
              <td class="col-protect">
                <div class="protect">
                  <UiSwitch
                    :model-value="env.protected"
                    :label="`${env.name || '该环境'} 为保护环境`"
                    @update:model-value="requestProtect(env)"
                  />
                  <UiTag v-if="env.protected" tone="warning" :icon="Shield">保护</UiTag>
                </div>
              </td>
              <td class="col-actions">
                <UiIconButton
                  :icon="Trash2"
                  :label="`删除环境 ${env.name}`"
                  @click="requestDelete(env)"
                />
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <section class="section" aria-labelledby="var-title">
        <header class="section-head">
          <h2 id="var-title">全局变量</h2>
          <p class="section-hint">所有项目都可引用；项目变量和环境覆盖优先。敏感变量只保存引用。</p>
          <UiButton :icon="Plus" compact @click="addVariable">新增变量</UiButton>
        </header>
        <table v-if="draft.globalVariables.length" class="table">
          <thead>
            <tr>
              <th scope="col">变量名</th>
              <th scope="col" class="col-source">来源</th>
              <th scope="col">值</th>
              <th scope="col" class="col-sensitive">敏感</th>
              <th scope="col" class="col-actions"><span class="sr-only">操作</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(v, i) in draft.globalVariables" :key="v.id">
              <td data-var-name>
                <UiTextInput
                  v-model="v.name"
                  mono
                  :invalid="!!at(`globalVariables[${i}].name`)"
                  :aria-label="`变量 ${i + 1} 名称`"
                />
                <p v-if="at(`globalVariables[${i}].name`)" class="cell-error">
                  {{ at(`globalVariables[${i}].name`) }}
                </p>
              </td>
              <td class="col-source">
                <UiSelect
                  :model-value="v.value.kind"
                  :options="sourceOptions(v)"
                  :aria-label="`${v.name || '变量'} 的来源`"
                  @update:model-value="setSource(v, $event)"
                />
              </td>
              <td>
                <UiTextInput
                  v-if="v.value.kind === 'literal'"
                  v-model="v.value.value"
                  mono
                  :invalid="!!at(`globalVariables[${i}].value`)"
                  :aria-label="`${v.name || '变量'} 的值`"
                />
                <UiTextInput
                  v-else-if="v.value.kind === 'envRef'"
                  v-model="v.value.name"
                  mono
                  placeholder="环境变量名"
                  :invalid="!!at(`globalVariables[${i}].value.name`)"
                  :aria-label="`${v.name || '变量'} 读取的环境变量名`"
                />
                <UiTag v-else tone="warning">需填写 · 凭据管理器将在阶段 5 接入</UiTag>
                <p
                  v-if="at(`globalVariables[${i}].value`) || at(`globalVariables[${i}].value.name`)"
                  class="cell-error"
                >
                  {{ at(`globalVariables[${i}].value`) ?? at(`globalVariables[${i}].value.name`) }}
                </p>
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
                <UiIconButton
                  :icon="Trash2"
                  :label="`删除变量 ${v.name}`"
                  @click="removeVariable(i)"
                />
              </td>
            </tr>
          </tbody>
        </table>
        <p v-else class="empty">尚无全局变量</p>
      </section>

      <section class="section" aria-labelledby="data-title">
        <header class="section-head">
          <h2 id="data-title">数据目录与版本</h2>
        </header>
        <UiNotice v-if="infoError" tone="failure">{{ infoError }}</UiNotice>
        <dl v-if="info" class="facts">
          <dt>数据目录</dt>
          <dd class="facts-dir">
            <code>{{ info.dataDir }}</code>
            <UiButton :icon="FolderOpen" compact @click="openDataDir">打开数据目录</UiButton>
          </dd>
          <dt>版本</dt>
          <dd class="num">{{ info.version }}</dd>
        </dl>
      </section>
    </div>

    <UiConfirm
      :open="!!protectTarget"
      :title="protectTarget?.protected ? '取消保护环境' : '设为保护环境'"
      :confirm-label="protectTarget?.protected ? '取消保护' : '设为保护环境'"
      @cancel="protectTarget = null"
      @confirm="applyProtect"
    >
      <p v-if="protectTarget?.protected">
        取消后，在 {{ protectTarget?.name }} 执行前不再要求键入环境名。已有的运行记录不受影响。
      </p>
      <p v-else>
        设为保护环境后，在
        {{ protectTarget?.name }} 执行前需要键入环境名确认。已有的运行记录不受影响。
      </p>
      <p class="muted">保存配置后生效。</p>
    </UiConfirm>

    <UiConfirm
      :open="!!deleteTarget"
      title="删除环境"
      confirm-label="删除环境"
      danger
      @cancel="deleteTarget = null"
      @confirm="applyDelete"
    >
      <p>删除环境 {{ deleteTarget?.name }}？没有服务器或项目引用它。保存配置后生效。</p>
    </UiConfirm>

    <UiDialog
      :open="!!blockedEnvironment"
      :title="`无法删除环境 ${blockedEnvironment?.name ?? ''}`"
      description="先移除下列引用，再删除该环境。"
      @cancel="blockedEnvironment = null"
    >
      <ul class="refs">
        <li v-for="(r, i) in blockingRefs" :key="i">{{ describeReference(r) }}</li>
      </ul>
      <template #footer>
        <UiButton @click="blockedEnvironment = null">关闭</UiButton>
      </template>
    </UiDialog>
  </PageLayout>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
  max-width: 1080px;
}

.section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.section-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-3);
}

.section-hint {
  flex: 1;
  min-width: 240px;
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.table td {
  height: auto;
  padding-top: var(--space-1);
  padding-bottom: var(--space-1);
  vertical-align: top;
}

.col-protect {
  width: 160px;
}

.col-source {
  width: 160px;
}

.col-sensitive {
  width: 64px;
  text-align: center;
}

.col-actions {
  width: 48px;
}

.protect {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  height: var(--input-height);
}

.col-sensitive :deep(.checkbox) {
  height: var(--input-height);
}

.cell-error {
  margin: var(--space-1) 0 0;
  color: var(--state-failure);
  font: var(--font-caption);
}

.empty {
  margin: 0;
  color: var(--text-secondary);
}

.elsewhere,
.refs {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-5);
}

.facts {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-2) var(--space-4);
  align-items: center;
  margin: 0;
}

.facts dt {
  color: var(--text-secondary);
}

.facts dd {
  margin: 0;
  user-select: text;
}

.facts-dir {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.muted {
  color: var(--text-secondary);
}
</style>
