<script setup lang="ts">
import { Trash2 } from '@lucide/vue'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import DraftNotices from '@/components/DraftNotices.vue'
import { boundTargetCount } from '@/components/projects/projectStats'
import RootField from '@/components/projects/RootField.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiField from '@/components/ui/UiField.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useProjectDraft } from '@/composables/projectDraft'
import { cloneWorkspace } from '@/composables/workspaceDraft'
import { useT } from '@/i18n'
import { errorText } from '@/i18n/message'
import { type Pipeline, type Project } from '@/ipc'
import { useRunStore } from '@/stores/run'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

// P05 project settings (IXD-14): name, description, root and default environment. Saved with
// the context bar's Save. Changing the default environment does not move target bindings.
const props = defineProps<{ project: Project; pipeline: Pipeline | null }>()

const store = useWorkspaceStore()
const run = useRunStore()
const toast = useToastStore()
const router = useRouter()
const page = ref<HTMLElement | null>(null)
const { project, fieldError, outside, error, discard } = useProjectDraft(
  'drafts.labels.projectSettings',
  page,
  () => props.project.id,
)
const elsewhere = outside(['name', 'description', 'root', 'defaultEnvironmentId'])

const environmentOptions = computed(() =>
  store.environments.map((e) => ({ value: e.id, label: e.name })),
)

const root = computed({
  get: () => project.value?.root ?? '',
  set: (value: string) => {
    if (project.value) project.value.root = value.trim() ? value : null
  },
})

// ---- Delete (IXD-06) -----------------------------------------------------------------------

const confirming = ref(false)
const deleting = ref(false)
const deleteError = ref<string | null>(null)
const deleteReason = computed(() => (run.active ? t('projects.deleteBlocked') : null))

async function applyDelete() {
  if (!store.workspace) return
  deleting.value = true
  deleteError.value = null
  try {
    const name = props.project.name
    const draft = cloneWorkspace(store.workspace)
    draft.projects = draft.projects.filter((p) => p.id !== props.project.id)
    // Unsaved settings of a project being deleted are dropped with it.
    discard()
    await store.save(draft)
    toast.show(t('projects.deleted', { name }))
    await router.replace({ name: 'projects' })
  } catch (e) {
    deleteError.value = errorText(e)
  } finally {
    deleting.value = false
    confirming.value = false
  }
}
</script>

<template>
  <div v-if="project" ref="page" class="settings">
    <DraftNotices :error="error" :issues="elsewhere" />

    <section class="form" :aria-label="t('projects.settings.basic')">
      <UiField
        v-slot="{ id, describedBy, invalid }"
        :label="t('common.name')"
        :hint="t('common.nameHint')"
        :error="fieldError('name')"
        required
      >
        <UiTextInput
          :id="id"
          v-model="project.name"
          :invalid="invalid"
          :described-by="describedBy"
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
          v-model="project.description"
          :rows="3"
          :invalid="invalid"
          :described-by="describedBy"
        />
      </UiField>
      <RootField v-model="root" :error="fieldError('root')" />
      <UiField
        v-slot="{ id, describedBy, invalid }"
        :label="t('projects.dialog.defaultEnvironment')"
        :hint="t('projects.settings.defaultEnvHint')"
        :error="fieldError('defaultEnvironmentId')"
      >
        <UiSelect
          :id="id"
          v-model="project.defaultEnvironmentId"
          class="env"
          :options="environmentOptions"
          :invalid="invalid"
          :described-by="describedBy"
        />
      </UiField>
    </section>

    <section class="danger" aria-labelledby="danger-title">
      <h2 id="danger-title">{{ t('projects.delete.confirm') }}</h2>
      <p>{{ t('projects.delete.section', { name: props.project.name }) }}</p>
      <p v-if="deleteError" class="error">{{ deleteError }}</p>
      <UiButton
        variant="danger"
        :icon="Trash2"
        :disabled-reason="deleteReason"
        @click="confirming = true"
      >
        {{ t('projects.delete.confirm') }}
      </UiButton>
    </section>

    <UiConfirm
      :open="confirming"
      :title="t('projects.delete.title', { name: props.project.name })"
      :confirm-label="t('projects.delete.confirm')"
      danger
      :busy="deleting"
      @cancel="confirming = false"
      @confirm="applyDelete"
    >
      <p>
        {{
          t('projects.delete.countsDiscard', {
            pipelines: props.project.pipelines.length,
            targets: boundTargetCount(props.project),
          })
        }}
      </p>
    </UiConfirm>
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
  max-width: 720px;
}

.form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

/* A description is prose, not a command: no 6-line minimum (UI-16 is for commands). */
.description :deep(.control--multiline) {
  min-height: 0;
}

.env {
  max-width: 280px;
}

.danger {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-4);
  border: 1px solid var(--state-failure);
  border-radius: var(--radius-surface);
}

.danger h2 {
  margin: 0;
  font: var(--font-section);
}

.danger p {
  margin: 0;
  color: var(--text-secondary);
}

.danger .error {
  color: var(--state-failure);
}
</style>
