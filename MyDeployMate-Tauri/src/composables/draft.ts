import { onBeforeUnmount, onMounted } from 'vue'

import { useDraftStore, type DraftSource } from '@/stores/drafts'

/** Register an editor's draft with the leave guard while the component is mounted. */
export function useDraftGuard(source: DraftSource) {
  const drafts = useDraftStore()
  let unregister: (() => void) | null = null
  onMounted(() => (unregister = drafts.register(source)))
  onBeforeUnmount(() => unregister?.())
}
