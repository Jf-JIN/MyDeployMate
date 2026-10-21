<script setup lang="ts">
import { onMounted, ref } from 'vue'

import { call, toAppError, type AppInfo } from '@/ipc'

const info = ref<AppInfo | null>(null)
const error = ref<string | null>(null)

onMounted(async () => {
  try {
    info.value = await call('app_info')
  } catch (e) {
    error.value = toAppError(e).message
  }
})
</script>

<template>
  <section>
    <h1>配置中心</h1>
    <dl v-if="info" class="facts">
      <dt>版本</dt>
      <dd class="num">{{ info.version }}</dd>
      <dt>数据目录</dt>
      <dd>
        <code>{{ info.dataDir }}</code>
      </dd>
    </dl>
    <p v-else-if="error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.facts {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-2) var(--space-4);
  margin-top: var(--space-4);
}

.facts dt {
  color: var(--text-secondary);
}

.facts dd {
  margin: 0;
  user-select: text;
}
</style>
