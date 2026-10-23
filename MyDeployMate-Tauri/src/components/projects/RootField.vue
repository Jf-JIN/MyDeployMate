<script setup lang="ts">
import { FolderOpen } from '@lucide/vue'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { computed, ref, watch } from 'vue'

import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { call } from '@/ipc'

// PRJ-01 / IXD-07: optional local project root with a folder picker. A path that does not
// exist is pointed out, not blocked; a purely remote project leaves it empty.
const model = defineModel<string>({ required: true })
defineProps<{ error?: string | null }>()

const exists = ref<boolean | null>(null)
let ticket = 0
watch(
  model,
  async (path) => {
    const mine = ++ticket
    exists.value = null
    if (!path.trim()) return
    const found = await call('local_dir_exists', { path: path.trim() })
    if (mine === ticket) exists.value = found
  },
  { immediate: true },
)

const hint = computed(() =>
  exists.value === false
    ? '该目录目前不存在，执行本地步骤前需要创建。'
    : '本地步骤的相对路径以此为准；纯远端项目可留空。',
)

async function browse() {
  const picked = await openFileDialog({
    title: '选择项目根目录',
    directory: true,
    multiple: false,
    defaultPath: model.value || undefined,
  })
  if (typeof picked === 'string') model.value = picked
}
</script>

<template>
  <UiField v-slot="{ id, describedBy, invalid }" label="根目录" :hint="hint" :error="error">
    <div class="root">
      <UiTextInput
        :id="id"
        v-model="model"
        mono
        placeholder="D:\workspace\app"
        :invalid="invalid"
        :described-by="describedBy"
      />
      <UiIconButton tonal :icon="FolderOpen" label="浏览根目录" @click="browse" />
    </div>
  </UiField>
</template>

<style scoped>
.root {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.root > :first-child {
  flex: 1;
  min-width: 0;
}
</style>
