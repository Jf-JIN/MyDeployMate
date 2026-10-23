<script setup lang="ts">
import { FolderOpen } from '@lucide/vue'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { computed, ref, watch } from 'vue'

import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useT } from '@/i18n'
import { call } from '@/ipc'

const t = useT()

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
  exists.value === false ? t('projects.root.missing') : t('projects.root.hint'),
)

async function browse() {
  const picked = await openFileDialog({
    title: t('projects.root.pick'),
    directory: true,
    multiple: false,
    defaultPath: model.value || undefined,
  })
  if (typeof picked === 'string') model.value = picked
}
</script>

<template>
  <UiField
    v-slot="{ id, describedBy, invalid }"
    :label="t('projects.root.label')"
    :hint="hint"
    :error="error"
  >
    <div class="root">
      <UiTextInput
        :id="id"
        v-model="model"
        mono
        placeholder="D:\workspace\app"
        :invalid="invalid"
        :described-by="describedBy"
      />
      <UiIconButton tonal :icon="FolderOpen" :label="t('projects.root.browse')" @click="browse" />
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
