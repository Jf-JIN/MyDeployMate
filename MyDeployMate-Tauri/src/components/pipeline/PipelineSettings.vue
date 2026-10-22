<script setup lang="ts">
import { Info } from '@lucide/vue'
import { computed, ref, useId } from 'vue'

import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import type { ExecutionOrder, Pipeline, PipelineFailurePolicy } from '@/ipc'

import { clearSkipServer, exampleSequence, skipServerUsers } from './pipeline'

// IXD-08 (v0.2): pipeline-level settings. "跳过该服务器" needs server-first order; switching
// back to step-first while it is in use lists the affected places and changes them on confirm.
const pipeline = defineModel<Pipeline>('pipeline', { required: true })
defineProps<{ errorAt: (field: string) => string | null }>()

const exampleId = useId()
const showExample = ref(false)

const orderOptions = [
  { value: 'stepFirst' as ExecutionOrder, label: '步骤优先' },
  { value: 'serverFirst' as ExecutionOrder, label: '服务器优先' },
]

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
  { value: 'stop' as PipelineFailurePolicy, label: '失败停止' },
  { value: 'continue' as PipelineFailurePolicy, label: '失败继续' },
  {
    value: 'skipServer' as PipelineFailurePolicy,
    label: '跳过该服务器',
    disabledReason:
      pipeline.value.executionOrder === 'serverFirst' ? undefined : '仅服务器优先可用',
  },
])

const policyHint = computed(
  () =>
    ({
      stop: '任一任务失败后不再开始后续任务。',
      continue: '任务失败后继续后续任务，整次运行最终记为失败。',
      skipServer:
        '某台服务器的任务失败后，跳过它的后续任务，其他服务器继续；整次运行最终记为失败。',
    })[pipeline.value.failurePolicy],
)

const sequence = computed(() => exampleSequence(pipeline.value.executionOrder))
</script>

<template>
  <section class="settings" aria-label="流程设置">
    <div class="order">
      <span class="label">执行顺序</span>
      <UiSegmented v-model="order" :options="orderOptions" label="执行顺序" />
      <UiIconButton
        :icon="Info"
        :label="showExample ? '收起执行顺序示例' : '查看执行顺序示例'"
        :aria-expanded="showExample"
        :aria-controls="exampleId"
        @click="showExample = !showExample"
      />
    </div>
    <div v-if="showExample" :id="exampleId" class="example">
      <p class="example-title">
        示例：两台服务器 A、B，步骤为构建（本地）、上传、重启。{{
          pipeline.executionOrder === 'serverFirst'
            ? '服务器优先：一台服务器做完全部远程步骤再到下一台。'
            : '步骤优先：每一步在所有服务器上完成后再进入下一步。'
        }}
      </p>
      <ol class="example-list">
        <li v-for="task in sequence" :key="task">{{ task }}</li>
      </ol>
    </div>
    <UiField
      v-slot="{ id, describedBy, invalid }"
      label="失败策略"
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
      title="切换为步骤优先"
      confirm-label="改为其他策略并切换"
      @confirm="confirmStepFirst"
      @cancel="pendingOrder = false"
    >
      <p class="confirm-text">“跳过该服务器”仅服务器优先可用。以下位置正在使用它：</p>
      <ul class="confirm-list">
        <li v-if="affected.pipeline">流程失败策略：将改为“失败停止”</li>
        <li v-for="step in affected.steps" :key="step.id">
          步骤 {{ step.name }}：将改为“继承流程”
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
