<script setup lang="ts">
import { Info } from '@lucide/vue'
import { computed, ref, useId } from 'vue'

import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import { useT } from '@/i18n'
import type { ExecutionOrder, Pipeline, PipelineFailurePolicy } from '@/ipc'

import { clearSkipServer, exampleSequence, skipServerUsers } from './pipeline'

const t = useT()

// IXD-08 (v0.2): pipeline-level settings. "跳过该服务器" needs server-first order; switching
// back to step-first while it is in use lists the affected places and changes them on confirm.
const pipeline = defineModel<Pipeline>('pipeline', { required: true })
defineProps<{ errorAt: (field: string) => string | null }>()

const exampleId = useId()
const showExample = ref(false)

const orderOptions = computed(() => [
  { value: 'stepFirst' as ExecutionOrder, label: t('pipelines.settings.stepFirst') },
  { value: 'serverFirst' as ExecutionOrder, label: t('pipelines.settings.serverFirst') },
])

const pendingOrder = ref(false)
const affected = computed(() => skipServerUsers(pipeline.value))
const order = computed({
  get: () => pipeline.value.executionOrder,
  set: (next: ExecutionOrder) => {
    const users = affected.value
    if (next === 'stepFirst' && (users.pipeline || users.steps.length)) {
      pendingOrder.value = true
      return
    }
    pipeline.value.executionOrder = next
  },
})

function confirmStepFirst() {
  clearSkipServer(pipeline.value)
  pipeline.value.executionOrder = 'stepFirst'
  pendingOrder.value = false
}

const policyOptions = computed(() => [
  { value: 'stop' as PipelineFailurePolicy, label: t('pipelines.settings.stop') },
  { value: 'continue' as PipelineFailurePolicy, label: t('pipelines.settings.continue') },
  {
    value: 'skipServer' as PipelineFailurePolicy,
    label: t('pipelines.settings.skipServer'),
    disabledReason:
      pipeline.value.executionOrder === 'serverFirst'
        ? undefined
        : t('pipelines.settings.serverFirstOnly'),
  },
])

const policyHint = computed(() => t(`pipelines.settings.hint.${pipeline.value.failurePolicy}`))

const sequence = computed(() => exampleSequence(pipeline.value.executionOrder))
</script>

<template>
  <section class="settings" :aria-label="t('pipelines.settings.label')">
    <div class="order">
      <span class="label">{{ t('pipelines.settings.order') }}</span>
      <UiSegmented v-model="order" :options="orderOptions" :label="t('pipelines.settings.order')" />
      <UiIconButton
        :icon="Info"
        :label="
          showExample ? t('pipelines.settings.hideExample') : t('pipelines.settings.showExample')
        "
        :aria-expanded="showExample"
        :aria-controls="exampleId"
        @click="showExample = !showExample"
      />
    </div>
    <div v-if="showExample" :id="exampleId" class="example">
      <p class="example-title">
        {{ t('pipelines.example.intro')
        }}{{
          pipeline.executionOrder === 'serverFirst'
            ? t('pipelines.example.serverFirst')
            : t('pipelines.example.stepFirst')
        }}
      </p>
      <ol class="example-list">
        <li v-for="task in sequence" :key="task">{{ task }}</li>
      </ol>
    </div>
    <UiField
      v-slot="{ id, describedBy, invalid }"
      :label="t('pipelines.settings.policy')"
      :hint="policyHint"
      :error="errorAt('failurePolicy')"
    >
      <UiSelect
        :id="id"
        v-model="pipeline.failurePolicy"
        :options="policyOptions"
        :invalid="invalid"
        :described-by="describedBy"
      />
    </UiField>

    <UiConfirm
      :open="pendingOrder"
      :title="t('pipelines.settings.switchTitle')"
      :confirm-label="t('pipelines.settings.switchConfirm')"
      @confirm="confirmStepFirst"
      @cancel="pendingOrder = false"
    >
      <p class="confirm-text">{{ t('pipelines.settings.switchBody') }}</p>
      <ul class="confirm-list">
        <li v-if="affected.pipeline">{{ t('pipelines.settings.switchPipeline') }}</li>
        <li v-for="step in affected.steps" :key="step.id">
          {{ t('pipelines.settings.switchStep', { name: step.name }) }}
        </li>
      </ul>
    </UiConfirm>
  </section>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding-bottom: var(--space-4);
  border-bottom: 1px solid var(--border-subtle);
}

.order {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.label {
  font: var(--font-label);
}

.example {
  padding: var(--space-3);
  border-radius: var(--radius-control);
  background: var(--surface-canvas);
  font: var(--font-caption);
}

.example-title {
  margin: 0 0 var(--space-2);
}

.example-list {
  margin: 0;
  padding-left: var(--space-5);
}

.confirm-text {
  margin: 0;
}

.confirm-list {
  margin: 0;
  padding-left: var(--space-5);
}
</style>
