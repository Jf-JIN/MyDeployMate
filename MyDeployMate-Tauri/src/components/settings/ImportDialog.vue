<script setup lang="ts">
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { FileUp } from '@lucide/vue'
import { computed, ref } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiTag from '@/components/ui/UiTag.vue'
import type { Tone } from '@/components/ui/types'
import { useT } from '@/i18n'
import { messageText } from '@/i18n/message'
import {
  call,
  toAppError,
  type Change,
  type DiffEntry,
  type DiffKind,
  type ImportApplied,
  type ImportPreview,
  type Issue,
} from '@/ipc'
import { useWorkspaceStore } from '@/stores/workspace'

import { groupChanges } from './importDiff'

const t = useT()

// D05 import (IXD-37, FLOW-08): pick a file, Rust validates and migrates it; a file that fails
// shows its issues and changes nothing. A valid one shows what the replacement changes and
// which local paths need fixing; confirming backs up the current configuration and replaces it
// atomically. Nothing runs.
const emit = defineEmits<{ imported: [result: ImportApplied] }>()

const store = useWorkspaceStore()

const open = ref(false)
const fileName = ref('')
const preview = ref<ImportPreview | null>(null)
const invalid = ref<Issue[]>([])
const error = ref<string | null>(null)
const busy = ref(false)

const groups = computed(() => (preview.value ? groupChanges(preview.value.changes) : []))
const currentVersion = computed(() => store.workspace?.schemaVersion ?? 0)

/** Ask for a file and preview it; the dialog opens only once a file is chosen. */
async function choose() {
  const path = await openFileDialog({
    multiple: false,
    directory: false,
    filters: [{ name: t('settings.exchange.fileFilter'), extensions: ['json'] }],
  })
  if (typeof path !== 'string') return
  fileName.value = path.split(/[\\/]/).pop() ?? path
  preview.value = null
  invalid.value = []
  error.value = null
  open.value = true
  busy.value = true
  try {
    preview.value = await call('import_preview', { path })
  } catch (e) {
    const err = toAppError(e)
    if (err.kind === 'validation' && err.validation) invalid.value = err.validation.issues
    else error.value = messageText(err.message)
  } finally {
    busy.value = false
  }
}

async function apply() {
  if (!preview.value) return
  busy.value = true
  error.value = null
  try {
    const result = await call('import_apply', { token: preview.value.token })
    store.workspace = result.workspace
    open.value = false
    emit('imported', result)
  } catch (e) {
    error.value = messageText(toAppError(e).message)
  } finally {
    busy.value = false
  }
}

function entryName(entry: DiffEntry): string {
  if (entry.kind === 'preferences') return t('settings.import.preferences')
  if (entry.parent)
    return t('settings.import.pipelineOf', { project: entry.parent, name: entry.name })
  return entry.name
}

const CHANGE_TONE: Record<Change, Tone> = {
  added: 'success',
  modified: 'warning',
  removed: 'failure',
}

const CHANGES: Change[] = ['added', 'modified', 'removed']

/** "新增 2 · 修改 1", leaving out changes that did not happen. */
function countsText(counts: Record<Change, number>) {
  return CHANGES.filter((c) => counts[c] > 0)
    .map((c) => t(`settings.import.count.${c}`, { n: counts[c] }))
    .join(' · ')
}

function kindLabel(kind: DiffKind) {
  return t(`settings.import.kinds.${kind}`)
}

defineExpose({ choose })
</script>

<template>
  <UiDialog
    :open="open"
    :title="t('settings.import.title')"
    :description="fileName"
    width="preview"
    :cancellable="!busy"
    @cancel="open = false"
  >
    <div class="import">
      <UiNotice v-if="error" tone="failure">{{ error }}</UiNotice>

      <template v-if="invalid.length">
        <UiNotice tone="failure">
          {{ t('settings.import.invalidTitle') }}
          <ul class="issues">
            <li v-for="(issue, i) in invalid" :key="i">
              <code class="issue-path">{{ issue.path || t('settings.import.fileLevel') }}</code>
              {{ messageText(issue.message) }}
            </li>
          </ul>
        </UiNotice>
        <p class="muted">{{ t('settings.import.invalidHint') }}</p>
      </template>

      <template v-else-if="preview">
        <p v-if="preview.fromVersion < currentVersion" class="muted">
          {{ t('settings.import.version', { from: preview.fromVersion, to: currentVersion }) }}
        </p>
        <template v-if="groups.length">
          <p>{{ t('settings.import.summary') }}</p>
          <section v-for="group in groups" :key="group.kind" class="group">
            <h3 class="group-head">
              <span>{{ kindLabel(group.kind) }}</span>
              <span class="group-counts num">{{ countsText(group.counts) }}</span>
            </h3>
            <ul class="entries">
              <li v-for="(entry, i) in group.entries" :key="i" class="entry">
                <UiTag :tone="CHANGE_TONE[entry.change]">
                  {{ t(`settings.import.change.${entry.change}`) }}
                </UiTag>
                <span>{{ entryName(entry) }}</span>
              </li>
            </ul>
          </section>
        </template>
        <p v-else class="muted">{{ t('settings.import.noChanges') }}</p>

        <UiNotice v-if="preview.missingPaths.length" tone="warning">
          {{ t('settings.import.missingTitle') }}
          <ul class="issues">
            <li v-for="(p, i) in preview.missingPaths" :key="i">
              {{
                p.kind === 'projectRoot'
                  ? t('settings.import.missingRoot', { owner: p.owner })
                  : t('settings.import.missingKey', { owner: p.owner })
              }}
              <code class="issue-path">{{ p.path }}</code>
            </li>
          </ul>
        </UiNotice>
        <p class="muted">{{ t('settings.import.backupNote') }}</p>
      </template>
    </div>

    <template #footer>
      <UiButton :disabled="busy" @click="open = false">{{ t('common.cancel') }}</UiButton>
      <UiButton v-if="invalid.length" variant="primary" :icon="FileUp" @click="choose">
        {{ t('settings.import.chooseAnother') }}
      </UiButton>
      <UiButton v-else-if="preview" variant="danger" :busy="busy" @click="apply">
        {{ t('settings.import.confirm') }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.import {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.muted {
  color: var(--text-secondary);
}

.issues {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-5);
}

.issue-path {
  margin-right: var(--space-2);
  font: var(--font-code);
  overflow-wrap: anywhere;
}

.group {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-surface);
}

.group-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-3);
  margin: 0;
  font: var(--font-label);
}

.group-counts {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.entries {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.entry {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
</style>
