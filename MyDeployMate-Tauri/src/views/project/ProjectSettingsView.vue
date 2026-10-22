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
import { toAppError, type Pipeline, type Project } from '@/ipc'
import { useRunStore } from '@/stores/run'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

// P05 project settings (IXD-14): name, description, root and default environment. Saved with
// the context bar's Save. Changing the default environment does not move target bindings.
const props = defineProps<{ project: Project; pipeline: Pipeline | null }>()

const store = useWorkspaceStore()
const run = useRunStore()
const toast = useToastStore()
const router = useRouter()
const page = ref<HTMLElement | null>(null)
const { project, fieldError, outside, error, discard } = useProjectDraft(
  '项目设置',
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
const deleteReason = computed(() => (run.active ? '有运行进行中，结束后才能删除项目' : null))

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
    toast.show(`已删除 ${name}`)
    await router.replace({ name: 'projects' })
  } catch (e) {
    deleteError.value = toAppError(e).message
  } finally {
    deleting.value = false
    confirming.value = false
  }
}
</script>

<template>
  <div v-if="project" ref="page" class="settings">
    <DraftNotices :error="error" :issues="elsewhere" />

    <section class="form" aria-label="基本信息">
      <UiField
        v-slot="{ id, describedBy, invalid }"
        label="名称"
        hint="1–64 个字符"
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
        label="说明"
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
        label="默认环境"
        hint="打开项目时选中的环境。修改它不会迁移已有的目标绑定。"
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
      <h2 id="danger-title">删除项目</h2>
      <p>
        删除
        {{ props.project.name }}
        的全部流程、变量和目标绑定。只删除本地配置，不会删除服务器上的目录或服务；运行历史会保留。
      </p>
      <p v-if="deleteError" class="error">{{ deleteError }}</p>
      <UiButton
        variant="danger"
        :icon="Trash2"
        :disabled-reason="deleteReason"
        @click="confirming = true"
      >
        删除项目
      </UiButton>
    </section>

    <UiConfirm
      :open="confirming"
      :title="`删除项目 ${props.project.name}`"
      confirm-label="删除项目"
      danger
      :busy="deleting"
      @cancel="confirming = false"
      @confirm="applyDelete"
    >
      <p>
        将删除 {{ props.project.pipelines.length }} 个流程和
        {{ boundTargetCount(props.project) }} 个目标绑定，未保存的设置也会丢弃。
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
