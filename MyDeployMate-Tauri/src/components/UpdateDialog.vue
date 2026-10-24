<script setup lang="ts">
import { Download } from '@lucide/vue'
import { computed, nextTick, ref, watch } from 'vue'

import { sizeText } from '@/components/run/upload'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import { useT } from '@/i18n'
import { messageText } from '@/i18n/message'
import { useRunStore } from '@/stores/run'
import { useUpdateStore } from '@/stores/update'

const t = useT()

// IXD-39, UI-34: versions, release notes, and "later" or "download and install". Nothing
// installs over an active run; the download can be cancelled until the installer starts.
const update = useUpdateStore()
const runs = useRunStore()

const info = computed(() => update.available)
const cancelButton = ref<InstanceType<typeof UiButton> | null>(null)
const installButton = ref<InstanceType<typeof UiButton> | null>(null)

// The footer swaps its buttons when the download starts or stops; keep focus in the dialog.
watch(
  () => update.phase === 'idle',
  async (idle) => {
    await nextTick()
    const el = (idle ? installButton : cancelButton).value?.$el as HTMLElement | undefined
    el?.focus()
  },
)
const busy = computed(() => update.phase !== 'idle')
const blocked = computed(() => (runs.active ? t('update.runActive') : null))

const progressText = computed(() => {
  const p = update.progress
  if (!p) return t('update.starting')
  return p.total === null
    ? t('update.downloadedOnly', { done: sizeText(p.downloaded) })
    : t('update.downloaded', { done: sizeText(p.downloaded), total: sizeText(p.total) })
})
const ratio = computed(() => {
  const p = update.progress
  return p?.total ? Math.min(p.downloaded / p.total, 1) : undefined
})
</script>

<template>
  <UiDialog
    :open="update.dialogOpen && !!info"
    :title="t('update.title')"
    :cancellable="update.phase !== 'installing'"
    @cancel="update.close()"
  >
    <template v-if="info">
      <dl class="versions">
        <dt>{{ t('update.current') }}</dt>
        <dd class="num">{{ info.currentVersion }}</dd>
        <dt>{{ t('update.new') }}</dt>
        <dd class="num">{{ info.version }}</dd>
      </dl>
      <h3 class="notes-title">{{ t('update.notes') }}</h3>
      <!-- Release notes are text from the release, shown as written and never as HTML. -->
      <p v-if="info.notes" class="notes">{{ info.notes }}</p>
      <p v-else class="notes notes--empty">{{ t('update.noNotes') }}</p>

      <div v-if="busy" class="progress" aria-live="polite">
        <progress :value="ratio" max="1" :aria-label="progressText" />
        <span class="num">
          {{ update.phase === 'installing' ? t('update.installing') : progressText }}
        </span>
      </div>
      <UiNotice v-else-if="update.error" tone="failure">{{ messageText(update.error) }}</UiNotice>
      <p v-else class="hint">{{ t('update.backupNote') }}</p>
    </template>

    <template #footer>
      <template v-if="busy">
        <UiButton
          ref="cancelButton"
          :disabled-reason="update.phase === 'installing' ? t('update.installing') : null"
          @click="update.cancel()"
        >
          {{ t('update.cancelDownload') }}
        </UiButton>
      </template>
      <template v-else>
        <UiButton @click="update.close()">{{ t('update.later') }}</UiButton>
        <UiButton
          ref="installButton"
          variant="primary"
          :icon="Download"
          :disabled-reason="blocked"
          @click="update.install()"
        >
          {{ t('update.install') }}
        </UiButton>
      </template>
    </template>
  </UiDialog>
</template>

<style scoped>
.versions {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-1) var(--space-4);
  margin: 0 0 var(--space-4);
}

.versions dt {
  color: var(--text-secondary);
}

.versions dd {
  margin: 0;
  font-weight: 600;
}

.notes-title {
  margin: 0 0 var(--space-1);
  font: var(--font-label);
}

.notes {
  max-height: 240px;
  margin: 0 0 var(--space-4);
  overflow-y: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.notes--empty,
.hint {
  color: var(--text-secondary);
}

.hint {
  margin: 0;
  font: var(--font-caption);
}

.progress {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.progress progress {
  flex: 1;
  height: 6px;
  accent-color: var(--action-primary);
}
</style>
