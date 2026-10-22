<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import { useDraftStore } from '@/stores/drafts'

// IXD-03: leaving with unsaved changes offers save and continue, discard, or stay.
const drafts = useDraftStore()
</script>

<template>
  <UiDialog
    :open="!!drafts.pending"
    title="有未保存的更改"
    width="credential"
    @cancel="drafts.answer('cancel')"
  >
    <p class="guard-text">离开前要保存这些更改吗？</p>
    <ul class="guard-list">
      <li v-for="label in drafts.pending?.labels ?? []" :key="label">{{ label }}</li>
    </ul>
    <template #footer>
      <UiButton @click="drafts.answer('cancel')">取消</UiButton>
      <UiButton @click="drafts.answer('discard')">放弃更改</UiButton>
      <UiButton variant="primary" @click="drafts.answer('save')">保存并继续</UiButton>
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
