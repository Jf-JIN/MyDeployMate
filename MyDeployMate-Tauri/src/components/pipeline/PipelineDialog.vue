<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { cloneWorkspace, useIssues } from '@/composables/workspaceDraft'
import { call, toAppError, type Pipeline, type Project } from '@/ipc'
import { useWorkspaceStore } from '@/stores/workspace'

export type PipelineDialogMode = 'create' | 'rename' | 'copy'

// IXD-08: create, rename or copy a pipeline. These save at once, so the opener settles any
// unsaved draft first (leave guard); a copy keeps the steps but gets new ids.
const props = defineProps<{
  mode: PipelineDialogMode | null
  project: Project
  source?: Pipeline | null
}>()
const emit = defineEmits<{ cancel: []; saved: [pipelineId: string] }>()

const store = useWorkspaceStore()
const { issues, at } = useIssues()
const error = ref<string | null>(null)
const saving = ref(false)
const name = ref('')

watch(
  () => props.mode,
  (mode) => {
    issues.value = []
    error.value = null
    name.value =
      mode === 'rename'
        ? (props.source?.name ?? '')
        : mode === 'copy'
          ? `${props.source?.name ?? ''} 副本`
          : ''
  },
  { immediate: true },
)

const title = computed(
  () =>
    ({
      create: '新建流程',
      rename: `重命名流程 ${props.source?.name ?? ''}`,
      copy: `复制流程 ${props.source?.name ?? ''}`,
    })[props.mode ?? 'create'],
)
const confirmLabel = computed(
  () => ({ create: '创建流程', rename: '重命名', copy: '复制流程' })[props.mode ?? 'create'],
)

const location = ref<string | null>(null)
const nameError = computed(() => (location.value ? at(`${location.value}.name`) : null))
const otherIssues = computed(() =>
  issues.value.filter((i) => !location.value || !i.path.startsWith(location.value)),
)

async function onSave() {
  if (!store.workspace || !props.mode) return
  error.value = null
  saving.value = true
  try {
    const draft = cloneWorkspace(store.workspace)
    const i = draft.projects.findIndex((p) => p.id === props.project.id)
    const project = draft.projects[i]
    if (!project) return
    let pipeline: Pipeline
    let j: number
    if (props.mode === 'rename') {
      j = project.pipelines.findIndex((p) => p.id === props.source?.id)
      const existing = project.pipelines[j]
      if (!existing) return
      existing.name = name.value
      pipeline = existing
    } else {
      pipeline =
        props.mode === 'copy' && props.source
          ? await call('copy_pipeline', { pipeline: props.source, name: name.value })
          : await call('new_pipeline', { name: name.value })
      // A copy goes right after its source; a new pipeline goes last.
      j =
        props.mode === 'copy'
          ? project.pipelines.findIndex((p) => p.id === props.source?.id) + 1
          : 0
      if (j <= 0) j = project.pipelines.length
      project.pipelines.splice(j, 0, pipeline)
    }
    location.value = `projects[${i}].pipelines[${j}]`
    await store.save(draft)
    emit('saved', pipeline.id)
  } catch (e) {
    const err = toAppError(e)
    if (err.kind === 'validation' && err.validation) issues.value = err.validation.issues
    else error.value = err.message
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDialog :open="!!mode" :title="title" width="credential" @cancel="emit('cancel')">
    <div v-if="mode" class="pipeline-form">
      <UiNotice v-if="error" tone="failure">{{ error }}</UiNotice>
      <UiNotice v-if="otherIssues.length" tone="warning">
        配置的其他位置还有问题，需要先修正才能保存：
        <ul class="issues">
          <li v-for="issue in otherIssues" :key="issue.path">{{ issue.message }}</li>
        </ul>
      </UiNotice>
      <p v-if="mode === 'copy'" class="note">复制全部步骤及其参数，步骤获得新的 ID。</p>
      <UiField
        v-slot="{ id, describedBy, invalid }"
        label="名称"
        hint="1–64 个字符，同一项目内不重复"
        :error="nameError"
        required
      >
        <UiTextInput
          :id="id"
          v-model="name"
          :invalid="invalid"
          :described-by="describedBy"
          autofocus
          @keydown.enter="onSave"
        />
      </UiField>
    </div>
    <template #footer>
      <UiButton :disabled="saving" @click="emit('cancel')">取消</UiButton>
      <UiButton variant="primary" :busy="saving" @click="onSave">{{ confirmLabel }}</UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.pipeline-form {
  display: flex;
  flex-direction: column;
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
