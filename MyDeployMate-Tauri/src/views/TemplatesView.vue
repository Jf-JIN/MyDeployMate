<script setup lang="ts">
import { Eye, LayoutTemplate, Plus } from '@lucide/vue'
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import {
  addTemplateVariables,
  missingVariables,
  templateName,
  templatePipeline,
  templatePurpose,
  templateSteps,
  uniquePipelineName,
} from '@/components/templates/templates'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { cloneWorkspace, focusFirstInvalid, useIssues } from '@/composables/workspaceDraft'
import { useT } from '@/i18n'
import { errorText, messageText } from '@/i18n/message'
import { call, toAppError, type PipelineTemplate, type Step } from '@/ipc'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

// P09 pipeline templates (IXD-35, PRJ-03): name, purpose, step count and a preview of the real
// steps; using one copies it into a project as a new, independent pipeline. Nothing runs and
// no targets change.
const store = useWorkspaceStore()
const toast = useToastStore()
const router = useRouter()

const templates = ref<PipelineTemplate[]>([])
const loadError = ref<string | null>(null)
onMounted(async () => {
  try {
    templates.value = await call('pipeline_templates')
  } catch (e) {
    loadError.value = errorText(e)
  }
})

/** What a step does, as its example command or path. */
function stepDetail(step: Step): string {
  const spec = step.spec
  switch (spec.type) {
    case 'local':
      return spec.command
    case 'upload':
      return `${spec.source} → ${spec.target}`
    case 'remote':
      return spec.command.mode === 'script'
        ? spec.command.script
        : [spec.command.executable, ...spec.command.args].join(' ')
    case 'confirm':
      return spec.prompt
  }
}

// ---- Preview -------------------------------------------------------------------------------

const previewing = ref<PipelineTemplate | null>(null)
const previewSteps = computed(() => (previewing.value ? templateSteps(previewing.value) : []))

// ---- Use -----------------------------------------------------------------------------------

const using = ref<PipelineTemplate | null>(null)
const projectId = ref('')
const name = ref('')
const saving = ref(false)
const error = ref<string | null>(null)
const body = ref<HTMLElement | null>(null)
const { issues, at } = useIssues()

const projectOptions = computed(() =>
  [...store.projects]
    .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN', { numeric: true }))
    .map((p) => ({ value: p.id, label: p.name })),
)
const project = computed(() => store.projects.find((p) => p.id === projectId.value))

function use(template: PipelineTemplate) {
  previewing.value = null
  using.value = template
  error.value = null
  issues.value = []
  projectId.value = projectOptions.value[0]?.value ?? ''
  name.value = uniquePipelineName(project.value, templateName(template.id))
}

// A new project may already have a pipeline with the suggested name.
watch(projectId, () => {
  if (using.value) name.value = uniquePipelineName(project.value, templateName(using.value.id))
})

const added = computed(() =>
  using.value && project.value && store.workspace
    ? missingVariables(using.value, project.value, store.workspace)
    : [],
)

/** Where the new pipeline lands in the draft, for field errors. */
const target = ref<{ project: number; pipeline: number } | null>(null)
const nameError = computed(() =>
  target.value
    ? at(`projects[${target.value.project}].pipelines[${target.value.pipeline}].name`)
    : null,
)

async function create() {
  const template = using.value
  if (!template || !store.workspace) return
  const draft = cloneWorkspace(store.workspace)
  const index = draft.projects.findIndex((p) => p.id === projectId.value)
  const into = draft.projects[index]
  if (!into) return
  saving.value = true
  error.value = null
  try {
    await addTemplateVariables(template, into, draft)
    const pipeline = templatePipeline(template, name.value)
    into.pipelines.push(pipeline)
    target.value = { project: index, pipeline: into.pipelines.length - 1 }
    await store.save(draft)
    using.value = null
    toast.show(t('templates.create.created', { name: pipeline.name }))
    await router.push({
      name: 'pipeline',
      params: { projectId: into.id, pipelineId: pipeline.id },
    })
  } catch (e) {
    const err = toAppError(e)
    if (err.kind === 'validation' && err.validation) {
      issues.value = err.validation.issues
      if (!nameError.value) error.value = messageText(err.validation.issues[0]!.message)
      await focusFirstInvalid(body.value)
    } else {
      error.value = messageText(err.message)
    }
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <PageLayout :title="t('templates.title')">
    <div class="templates">
      <UiNotice v-if="loadError" tone="failure">{{ loadError }}</UiNotice>
      <p class="intro">{{ t('templates.intro') }}</p>
      <ul v-if="templates.length" class="cards">
        <li v-for="tpl in templates" :key="tpl.id" class="card">
          <div class="card-head">
            <LayoutTemplate :size="20" :stroke-width="1.75" aria-hidden="true" />
            <h2 class="card-title">{{ templateName(tpl.id) }}</h2>
          </div>
          <p class="card-purpose">{{ templatePurpose(tpl.id) }}</p>
          <p class="card-meta num">
            {{
              tpl.pipeline.steps.length
                ? t('templates.stepCount', { n: tpl.pipeline.steps.length })
                : t('templates.noSteps')
            }}
          </p>
          <div class="card-actions">
            <UiButton
              :icon="Eye"
              :aria-label="t('templates.previewItem', { name: templateName(tpl.id) })"
              @click="previewing = tpl"
            >
              {{ t('templates.preview') }}
            </UiButton>
            <UiButton
              variant="tonal"
              :icon="Plus"
              :aria-label="t('templates.useItem', { name: templateName(tpl.id) })"
              @click="use(tpl)"
            >
              {{ t('templates.use') }}
            </UiButton>
          </div>
        </li>
      </ul>
      <EmptyState v-else-if="!loadError" :title="t('templates.title')" :icon="LayoutTemplate" />
    </div>

    <UiDialog
      :open="!!previewing"
      :title="previewing ? t('templates.previewTitle', { name: templateName(previewing.id) }) : ''"
      width="preview"
      @cancel="previewing = null"
    >
      <div v-if="previewing" class="preview">
        <p class="muted">{{ templatePurpose(previewing.id) }}</p>
        <UiNotice v-if="previewing.needsRepoAccess" tone="warning">
          {{ t('templates.repoAccess') }}
        </UiNotice>
        <ol v-if="previewSteps.length" class="steps">
          <li v-for="(step, i) in previewSteps" :key="step.id" class="step">
            <div class="step-head">
              <span class="step-no num">{{ i + 1 }}</span>
              <span class="step-name">{{ step.name }}</span>
              <UiTag>{{ t(`steps.kind.${step.spec.type}`) }}</UiTag>
            </div>
            <pre class="step-detail">{{ stepDetail(step) }}</pre>
          </li>
        </ol>
        <p v-else class="muted">{{ t('templates.previewEmpty') }}</p>
        <p v-if="previewSteps.length" class="muted">{{ t('templates.examples') }}</p>
      </div>
      <template #footer>
        <UiButton @click="previewing = null">{{ t('common.close') }}</UiButton>
        <UiButton v-if="previewing" variant="primary" :icon="Plus" @click="use(previewing)">
          {{ t('templates.use') }}
        </UiButton>
      </template>
    </UiDialog>

    <UiDialog
      :open="!!using"
      :title="using ? t('templates.create.title', { name: templateName(using.id) }) : ''"
      :cancellable="!saving"
      @cancel="using = null"
    >
      <div v-if="using" ref="body" class="form">
        <UiNotice v-if="error" tone="failure">{{ error }}</UiNotice>
        <template v-if="projectOptions.length">
          <UiField v-slot="{ id }" :label="t('templates.create.project')" required>
            <UiSelect :id="id" v-model="projectId" :options="projectOptions" />
          </UiField>
          <UiField
            v-slot="{ id, describedBy, invalid }"
            :label="t('templates.create.name')"
            :error="nameError"
            required
          >
            <UiTextInput
              :id="id"
              v-model="name"
              :invalid="invalid"
              :described-by="describedBy"
              autofocus
            />
          </UiField>
          <p v-if="added.length" class="muted">
            {{ t('templates.create.variables', { names: added.join('、') }) }}
          </p>
          <UiNotice v-if="using.needsRepoAccess" tone="warning">
            {{ t('templates.repoAccess') }}
          </UiNotice>
          <p class="muted">{{ t('templates.create.notRun') }}</p>
        </template>
        <p v-else class="muted">{{ t('templates.create.noProjects') }}</p>
      </div>
      <template #footer>
        <UiButton :disabled="saving" @click="using = null">{{ t('common.cancel') }}</UiButton>
        <UiButton
          variant="primary"
          :busy="saving"
          :disabled-reason="projectOptions.length ? null : t('templates.create.noProjects')"
          @click="create"
        >
          {{ t('templates.create.confirm') }}
        </UiButton>
      </template>
    </UiDialog>
  </PageLayout>
</template>

<style scoped>
.templates {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.intro,
.muted {
  color: var(--text-secondary);
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--space-4);
  margin: 0;
  padding: 0;
  list-style: none;
}

.card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-4);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-surface);
  background: var(--surface-panel);
}

.card-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--action-primary);
}

.card-title {
  color: var(--text-primary);
  font: var(--font-label);
  font-size: 16px;
}

.card-purpose {
  flex: 1;
}

.card-meta {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.card-actions {
  display: flex;
  gap: var(--space-2);
  margin-top: var(--space-2);
}

.preview,
.form {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.steps {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.step {
  padding: var(--space-3);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-surface);
}

.step-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.step-no {
  color: var(--text-secondary);
}

.step-name {
  font-weight: 500;
}

.step-detail {
  margin: var(--space-2) 0 0;
  font: var(--font-code);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>
