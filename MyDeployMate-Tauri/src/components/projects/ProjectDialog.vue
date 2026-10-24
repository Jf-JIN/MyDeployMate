<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { cloneWorkspace, focusFirstInvalid, useIssues } from '@/composables/workspaceDraft'
import { useT } from '@/i18n'
import { messageText } from '@/i18n/message'
import {
  addTemplateVariables,
  templateName,
  templatePipeline,
} from '@/components/templates/templates'
import { call, toAppError, type PipelineTemplate, type Project, type TemplateId } from '@/ipc'
import { useWorkspaceStore } from '@/stores/workspace'

import RootField from './RootField.vue'

const t = useT()

// IXD-07: create a project (with its first, empty pipeline) or copy one. A copy keeps
// pipelines and variables but not target bindings or credentials, and starts with no name.
const props = defineProps<{ mode: 'create' | 'copy' | null; source?: Project | null }>()
const emit = defineEmits<{ cancel: []; saved: [project: Project] }>()

const store = useWorkspaceStore()
const body = ref<HTMLElement | null>(null)
const { issues, at } = useIssues()
const error = ref<string | null>(null)
const saving = ref(false)

const name = ref('')
const description = ref('')
const root = ref('')
const environmentId = ref('')
const template = ref<TemplateId>('blank')
const templates = ref<PipelineTemplate[]>([])
onMounted(async () => {
  templates.value = await call('pipeline_templates').catch(() => [])
})

watch(
  () => props.mode,
  (mode) => {
    issues.value = []
    error.value = null
    name.value = ''
    template.value = 'blank'
    description.value = mode === 'copy' ? (props.source?.description ?? '') : ''
    root.value = mode === 'copy' ? (props.source?.root ?? '') : ''
    environmentId.value =
      (mode === 'copy' ? props.source?.defaultEnvironmentId : undefined) ??
      store.environments[0]?.id ??
      ''
  },
  { immediate: true },
)

const title = computed(() =>
  props.mode === 'copy'
    ? t('projects.dialog.copyTitle', { name: props.source?.name ?? '' })
    : t('projects.dialog.createTitle'),
)
const environmentOptions = computed(() =>
  store.environments.map((e) => ({ value: e.id, label: e.name })),
)
// PRJ-03: the first pipeline can start from a P09 template; blank keeps the usual name.
const templateOptions = computed(() =>
  templates.value.length
    ? templates.value.map((tpl) => ({ value: tpl.id, label: templateName(tpl.id) }))
    : [{ value: 'blank' as TemplateId, label: t('projects.dialog.blankTemplate') }],
)

const index = ref(-1)
function fieldError(field: string) {
  return index.value < 0 ? null : at(`projects[${index.value}].${field}`)
}
const otherIssues = computed(() =>
  issues.value.filter((i) => !i.path.startsWith(`projects[${index.value}]`)),
)

async function build(): Promise<Project | null> {
  if (!store.workspace) return null
  if (props.mode === 'copy' && props.source) {
    const copy = await call('copy_project', {
      draft: cloneWorkspace(store.workspace),
      projectId: props.source.id,
      name: name.value,
    })
    return { ...copy, description: description.value, root: root.value.trim() || null }
  }
  const project = await call('new_project', {
    defaultEnvironmentId: environmentId.value,
    pipelineName: t('projects.dialog.firstPipeline'),
  })
  const tpl = templates.value.find((x) => x.id === template.value)
  if (tpl && tpl.id !== 'blank') {
    project.pipelines = [templatePipeline(tpl, templateName(tpl.id))]
    await addTemplateVariables(tpl, project, store.workspace)
  }
  return {
    ...project,
    name: name.value,
    description: description.value,
    root: root.value.trim() || null,
    defaultEnvironmentId: environmentId.value,
  }
}

// After a failed save, re-check as the user types so fixed fields clear (UI-19).
let validating = 0
watch([name, description, root, environmentId], async () => {
  if (!issues.value.length || !store.workspace) return
  const ticket = ++validating
  const project = await build()
  if (!project) return
  const draft = cloneWorkspace(store.workspace)
  draft.projects.push(project)
  const report = await call('validate_workspace_draft', { draft })
  if (ticket === validating) issues.value = report.issues
})

async function onSave() {
  if (!store.workspace) return
  error.value = null
  saving.value = true
  try {
    const project = await build()
    if (!project) return
    const draft = cloneWorkspace(store.workspace)
    draft.projects.push(project)
    index.value = draft.projects.length - 1
    await store.save(draft)
    emit('saved', project)
  } catch (e) {
    const err = toAppError(e)
    if (err.kind === 'validation' && err.validation) {
      issues.value = err.validation.issues
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
  <UiDialog :open="!!mode" :title="title" @cancel="emit('cancel')">
    <div v-if="mode" ref="body" class="project-form">
      <UiNotice v-if="error" tone="failure">{{ error }}</UiNotice>
      <UiNotice v-if="otherIssues.length" tone="warning">
        {{ t('common.issuesElsewhereShort') }}
        <ul class="issues">
          <li v-for="issue in otherIssues" :key="issue.path">{{ messageText(issue.message) }}</li>
        </ul>
      </UiNotice>
      <p v-if="mode === 'copy'" class="note">
        {{ t('projects.dialog.copyNote') }}
      </p>

      <UiField
        v-slot="{ id, describedBy, invalid }"
        :label="t('common.name')"
        :hint="t('common.nameHint')"
        :error="fieldError('name')"
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
      <UiField
        v-slot="{ id, describedBy, invalid }"
        :label="t('common.description')"
        class="description"
        :error="fieldError('description')"
      >
        <UiTextarea
          :id="id"
          v-model="description"
          :rows="2"
          :invalid="invalid"
          :described-by="describedBy"
        />
      </UiField>
      <RootField v-model="root" :error="fieldError('root')" />
      <div class="grid">
        <UiField
          v-slot="{ id, describedBy, invalid }"
          :label="t('projects.dialog.defaultEnvironment')"
          :error="fieldError('defaultEnvironmentId')"
        >
          <UiSelect
            :id="id"
            v-model="environmentId"
            :options="environmentOptions"
            :invalid="invalid"
            :described-by="describedBy"
          />
        </UiField>
        <UiField
          v-if="mode === 'create'"
          v-slot="{ id }"
          :label="t('projects.dialog.template')"
          :hint="t('projects.dialog.templateHint')"
        >
          <UiSelect :id="id" v-model="template" :options="templateOptions" />
        </UiField>
      </div>
    </div>

    <template #footer>
      <UiButton :disabled="saving" @click="emit('cancel')">{{ t('common.cancel') }}</UiButton>
      <UiButton variant="primary" :busy="saving" @click="onSave">
        {{ mode === 'copy' ? t('projects.dialog.copy') : t('projects.dialog.create') }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.project-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

/* A description is prose, not a command: no 6-line minimum (UI-16 is for commands). */
.description :deep(.control--multiline) {
  min-height: 0;
}

.grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-4);
}

.note {
  margin: 0;
  color: var(--text-secondary);
}

.issues {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-5);
}
</style>
