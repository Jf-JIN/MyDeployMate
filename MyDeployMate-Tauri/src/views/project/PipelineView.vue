<script setup lang="ts">
import { Copy, Ellipsis, MousePointerClick, Pencil, Plus, Trash2 } from '@lucide/vue'
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import DraftNotices from '@/components/DraftNotices.vue'
import EmptyState from '@/components/EmptyState.vue'
import PipelineDialog, { type PipelineDialogMode } from '@/components/pipeline/PipelineDialog.vue'
import PipelineSettings from '@/components/pipeline/PipelineSettings.vue'
import StepEditor from '@/components/pipeline/StepEditor.vue'
import StepList from '@/components/pipeline/StepList.vue'
import { provideVariableMenu } from '@/components/pipeline/variableMenu'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiMenu from '@/components/ui/UiMenu.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import type { MenuEntry } from '@/components/ui/types'
import { useProjectDraft } from '@/composables/projectDraft'
import { cloneWorkspace, focusFirstInvalid } from '@/composables/workspaceDraft'
import { useT } from '@/i18n'
import { errorText, issueText } from '@/i18n/message'
import {
  call,
  type BuiltinVariable,
  type Pipeline,
  type Project,
  type Server,
  type VariableView,
} from '@/ipc'
import { editorField } from '@/components/preview/preview'
import { useDraftStore } from '@/stores/drafts'
import { usePreviewStore } from '@/stores/preview'
import { useRunStore } from '@/stores/run'
import { useSessionStore } from '@/stores/session'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

// P02 pipeline editor (IXD-08 – IXD-11): pipeline settings and steps on the left, the selected
// step's parameters on the right. Step edits are a page draft saved from the context bar;
// pipeline create / rename / copy / delete save at once after the leave guard.
const props = defineProps<{ project: Project; pipeline: Pipeline | null }>()

const router = useRouter()
const store = useWorkspaceStore()
const session = useSessionStore()
const drafts = useDraftStore()
const run = useRunStore()
const page = ref<HTMLElement | null>(null)
const { draft, project, prefix, issues, under, error } = useProjectDraft(
  'drafts.labels.pipeline',
  page,
  () => props.project.id,
)

const pipelineIndex = computed(
  () => project.value?.pipelines.findIndex((p) => p.id === props.pipeline?.id) ?? -1,
)
const current = computed(() => project.value?.pipelines[pipelineIndex.value] ?? null)
const path = computed(() => `${prefix.value}.pipelines[${pipelineIndex.value}]`)

function pipelineError(field: string) {
  return issueText(issues.value.find((i) => i.path === `${path.value}.${field}`))
}

// ---- Steps ---------------------------------------------------------------------------------

const selectedId = ref<string | null>(null)
watch(
  () => props.pipeline?.id,
  () => (selectedId.value = current.value?.steps[0]?.id ?? null),
  { immediate: true },
)
// The first load can land after mount.
watch(current, (p) => {
  if (p && !p.steps.some((s) => s.id === selectedId.value)) {
    selectedId.value = p.steps[0]?.id ?? null
  }
})

const stepIndex = computed(
  () => current.value?.steps.findIndex((s) => s.id === selectedId.value) ?? -1,
)
const step = computed(() => current.value?.steps[stepIndex.value] ?? null)

// Preflight issues from the publish preview, until the draft changes (IXD-24).
const preview = usePreviewStore()
function previewIssue(stepId: string | undefined, field?: string) {
  return preview.stepIssues.find(
    (i) => i.object?.id === stepId && (!field || editorField(`.${i.field}`) === `.${field}`),
  )
}

function hasIssue(k: number) {
  return (
    under(`${path.value}.steps[${k}]`).length > 0 || !!previewIssue(current.value?.steps[k]?.id)
  )
}
function stepError(field: string) {
  return issueText(
    issues.value.find((i) => i.path === `${path.value}.steps[${stepIndex.value}].${field}`) ??
      previewIssue(step.value?.id, field),
  )
}

watch(
  () => preview.focusStepId,
  async (id) => {
    if (!id || !current.value?.steps.some((s) => s.id === id)) return
    selectedId.value = id
    preview.focusStepId = null
    await nextTick()
    await focusFirstInvalid(page.value?.querySelector('.right') as HTMLElement | null)
  },
  { immediate: true },
)
watch(draft, () => preview.clear(), { deep: true })

// Issues this page cannot show next to a field or a step row.
const elsewhere = computed(() =>
  issues.value.filter(
    (i) => !i.path.startsWith(`${path.value}.steps[`) && i.path !== `${path.value}.failurePolicy`,
  ),
)

const environmentId = computed(
  () => session.environment[props.project.id] ?? props.project.defaultEnvironmentId,
)
const targets = computed<Server[]>(() => {
  const ids = project.value?.targets.find((t) => t.environmentId === environmentId.value)
  const byId = new Map(store.servers.map((s) => [s.id, s]))
  return (ids?.serverIds ?? []).map((id) => byId.get(id)).filter((s): s is Server => !!s)
})

// ---- Variable menu (IXD-11) ----------------------------------------------------------------

const variables = ref<VariableView[]>([])
const builtins = ref<BuiltinVariable[]>([])
onMounted(async () => {
  builtins.value = await call('builtin_variables').catch(() => [])
})
let ticket = 0
let timer: ReturnType<typeof setTimeout> | undefined
watch(
  [() => draft.value?.globalVariables, () => project.value?.variables, environmentId],
  () => {
    clearTimeout(timer)
    timer = setTimeout(async () => {
      if (!draft.value) return
      const mine = ++ticket
      const list = await call('variable_views', {
        draft: draft.value,
        projectId: props.project.id,
        environmentId: environmentId.value,
      }).catch(() => [])
      if (mine === ticket) variables.value = list
    }, 150)
  },
  { deep: true, immediate: true },
)
provideVariableMenu({
  variables,
  builtins,
  environmentName: computed(() => store.environmentName(environmentId.value)),
})

// ---- Pipeline actions (IXD-08) -------------------------------------------------------------

const dialog = ref<PipelineDialogMode | null>(null)
const deleting = ref(false)
const deleteError = ref<string | null>(null)
const busy = ref(false)

const pipelineEntries = computed<MenuEntry[]>(() => [
  { key: 'create', label: t('pipelines.view.create'), icon: Plus },
  { key: 'rename', label: t('pipelines.view.rename'), icon: Pencil },
  { key: 'copy', label: t('pipelines.view.copy'), icon: Copy },
  {
    key: 'delete',
    label: t('pipelines.view.delete'),
    icon: Trash2,
    danger: true,
    separator: true,
    disabledReason: props.project.pipelines.length <= 1 ? t('pipelines.view.lastOne') : undefined,
  },
])

async function onPipelineAction(key: string) {
  // These save immediately, so unsaved step edits are settled first.
  if (!(await drafts.confirmLeave())) return
  if (key === 'delete') {
    deleteError.value = null
    deleting.value = true
  } else {
    dialog.value = key as PipelineDialogMode
  }
}

function openPipeline(id: string) {
  dialog.value = null
  void router.push({ name: 'pipeline', params: { projectId: props.project.id, pipelineId: id } })
}

async function deletePipeline() {
  if (!store.workspace || !props.pipeline) return
  busy.value = true
  try {
    const next = cloneWorkspace(store.workspace)
    const p = next.projects.find((x) => x.id === props.project.id)
    if (!p) return
    const at = p.pipelines.findIndex((x) => x.id === props.pipeline?.id)
    p.pipelines.splice(at, 1)
    const neighbour = p.pipelines[at] ?? p.pipelines[at - 1]
    await store.save(next)
    deleting.value = false
    if (neighbour) openPipeline(neighbour.id)
  } catch (e) {
    deleteError.value = errorText(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div v-if="project && current" ref="page" class="pipeline-page">
    <DraftNotices :error="error" :issues="elsewhere" />
    <UiNotice v-if="run.active">
      {{ t('pipelines.view.activeRun') }}
    </UiNotice>

    <div class="columns">
      <div class="left">
        <header class="pipeline-head">
          <h2 class="pipeline-name" :title="current.name">{{ current.name }}</h2>
          <span class="step-count">{{
            t('pipelines.view.stepCount', { n: current.steps.length })
          }}</span>
          <UiMenu
            :label="t('pipelines.view.actions')"
            :icon="Ellipsis"
            icon-only
            align="end"
            :entries="pipelineEntries"
            @select="onPipelineAction"
          />
        </header>
        <PipelineSettings :pipeline="current" :error-at="pipelineError" />
        <StepList v-model:selected-id="selectedId" :steps="current.steps" :has-issue="hasIssue" />
      </div>

      <div class="right">
        <StepEditor
          v-if="step"
          :key="step.id"
          :step="step"
          :pipeline="current"
          :targets="targets"
          :error-at="stepError"
        />
        <EmptyState
          v-else
          :title="t('pipelines.view.noStep')"
          :detail="t('pipelines.view.noStepDetail')"
          :icon="MousePointerClick"
          compact
        />
      </div>
    </div>

    <PipelineDialog
      :mode="dialog"
      :project="props.project"
      :source="props.pipeline"
      @cancel="dialog = null"
      @saved="openPipeline"
    />
    <UiConfirm
      :open="deleting"
      :title="t('pipelines.view.deleteTitle', { name: current.name })"
      :confirm-label="t('pipelines.view.delete')"
      danger
      :busy="busy"
      @confirm="deletePipeline"
      @cancel="deleting = false"
    >
      <UiNotice v-if="deleteError" tone="failure">{{ deleteError }}</UiNotice>
      <p class="confirm-text">
        {{ t('pipelines.view.deleteBody', { name: current.name, n: current.steps.length }) }}
      </p>
    </UiConfirm>
  </div>
</template>

<style scoped>
.pipeline-page {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--space-3);
  min-height: 0;
}

/* UI-11: list ≥ 320 px, parameters ≥ 420 px, about 44:56. */
.columns {
  display: grid;
  flex: 1;
  grid-template-columns: minmax(320px, 44fr) minmax(420px, 56fr);
  gap: var(--space-4);
  min-height: 0;
}

/* Both columns scroll; 4 px of padding, taken back with a negative margin, keeps the focus halo
   of edge fields from being clipped (UI-17). */
.left,
.right {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
  min-height: 0;
  margin: -4px;
  padding: 4px;
  overflow-y: auto;
}

.right {
  margin-left: 0;
  padding-left: var(--space-4);
  border-left: 1px solid var(--border-subtle);
}

.pipeline-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.pipeline-name {
  margin: 0;
  overflow: hidden;
  font: var(--font-section);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.step-count {
  flex: 1;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.confirm-text {
  margin: 0;
}
</style>
