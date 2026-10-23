<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import { useT } from '@/i18n'
import { useDraftStore } from '@/stores/drafts'

const t = useT()

// IXD-03: leaving with unsaved changes offers save and continue, discard, or stay.
const drafts = useDraftStore()
</script>

<template>
  <UiDialog
    :open="!!drafts.pending"
    :title="t('drafts.guard.title')"
    width="credential"
    @cancel="drafts.answer('cancel')"
  >
    <p class="guard-text">
      {{
        drafts.pending?.discardable === false
          ? t('drafts.guard.mustSave')
          : t('drafts.guard.askSave')
      }}
    </p>
    <ul class="guard-list">
      <li v-for="label in drafts.pending?.labels ?? []" :key="label">{{ t(label) }}</li>
    </ul>
    <template #footer>
      <UiButton @click="drafts.answer('cancel')">{{ t('common.cancel') }}</UiButton>
      <UiButton v-if="drafts.pending?.discardable !== false" @click="drafts.answer('discard')">
        {{ t('drafts.guard.discard') }}
      </UiButton>
      <UiButton variant="primary" @click="drafts.answer('save')">
        {{ t('drafts.guard.saveAndContinue') }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.guard-text {
  margin: 0;
}

.guard-list {
  margin: var(--space-2) 0 0;
  padding-left: var(--space-5);
  overflow-wrap: anywhere;
}
</style>
