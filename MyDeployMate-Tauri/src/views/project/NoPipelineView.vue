<script setup lang="ts">
import { Plus, Workflow } from '@lucide/vue'
import { ref } from 'vue'
import { useRouter } from 'vue-router'

import EmptyState from '@/components/EmptyState.vue'
import PipelineDialog from '@/components/pipeline/PipelineDialog.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { useT } from '@/i18n'
import type { Pipeline, Project } from '@/ipc'

const t = useT()

// A project without pipelines (validation normally prevents this, e.g. after an import).
const props = defineProps<{ project: Project; pipeline: Pipeline | null }>()

const router = useRouter()
const creating = ref(false)

function onSaved(pipelineId: string) {
  creating.value = false
  void router.push({ name: 'pipeline', params: { projectId: props.project.id, pipelineId } })
}
</script>

<template>
  <EmptyState
    :title="t('pipelines.empty.title')"
    :detail="t('pipelines.empty.detail')"
    :icon="Workflow"
  >
    <UiButton :icon="Plus" variant="primary" @click="creating = true">
      {{ t('tree.newPipeline') }}
    </UiButton>
  </EmptyState>
  <PipelineDialog
    :mode="creating ? 'create' : null"
    :project="project"
    @cancel="creating = false"
    @saved="onSaved"
  />
</template>
