import { computed, nextTick, ref, shallowRef, toRaw, watch, type Ref } from 'vue'

import { translate, type MessageKey } from '@/i18n'
import { issueText } from '@/i18n/message'
import { call, toAppError, type AppError, type Issue, type Workspace } from '@/ipc'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

import { useDraftGuard } from './draft'

/** Deep copy of a workspace, also from a reactive draft. */
export function cloneWorkspace(ws: Workspace): Workspace {
  return structuredClone(toRaw(ws))
}

/** Move focus to the first field marked invalid inside `root` (UI-19). */
export async function focusFirstInvalid(root: HTMLElement | null | undefined) {
  await nextTick()
  root?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus()
}

/**
 * Validation issues keyed by JSON path, as reported by Rust (`validate_config`, UI-19).
 * The UI only places them; it never re-implements the rules.
 */
export function useIssues() {
  const issues = shallowRef<Issue[]>([])

  function at(path: string): string | null {
    return issueText(issues.value.find((i) => i.path === path))
  }

  function under(prefix: string): Issue[] {
    return issues.value.filter((i) => i.path === prefix || i.path.startsWith(`${prefix}.`))
  }

  return { issues, at, under }
}

/**
 * A page-level draft of the whole workspace with explicit save (IXD-03): dirty tracking, the
 * leave guard, Rust validation on save, and live re-validation after a failed save so fixed
 * fields clear their errors.
 */
export function useWorkspaceDraft(label: MessageKey, root: Ref<HTMLElement | null>) {
  const store = useWorkspaceStore()
  const toast = useToastStore()
  const draft = ref<Workspace | null>(store.workspace ? cloneWorkspace(store.workspace) : null)
  const { issues, at, under } = useIssues()
  const error = ref<AppError | null>(null)
  const attempted = ref(false)

  const dirty = computed(
    () =>
      !!draft.value &&
      !!store.workspace &&
      JSON.stringify(draft.value) !== JSON.stringify(store.workspace),
  )

  // Pick up the first load and saves made elsewhere while this page has no changes. Compare
  // with the previous workspace: against the new one an unchanged draft already looks dirty.
  watch(
    () => store.workspace,
    (ws, before) => {
      const unchanged =
        !draft.value || !before || JSON.stringify(draft.value) === JSON.stringify(before)
      if (ws && unchanged) draft.value = cloneWorkspace(ws)
    },
  )

  let validating = 0
  watch(
    draft,
    async (value) => {
      if (!attempted.value || !value) return
      const ticket = ++validating
      const report = await call('validate_workspace_draft', { draft: value })
      if (ticket === validating) issues.value = report.issues
    },
    { deep: true },
  )

  async function save(): Promise<boolean> {
    if (!draft.value) return true
    error.value = null
    try {
      const saved = await store.save(draft.value)
      draft.value = cloneWorkspace(saved)
      issues.value = []
      attempted.value = false
      toast.show(translate('common.saved'))
      return true
    } catch (e) {
      const err = toAppError(e)
      attempted.value = true
      if (err.kind === 'validation' && err.validation) {
        issues.value = err.validation.issues
        await focusFirstInvalid(root.value)
      } else {
        error.value = err
      }
      return false
    }
  }

  function discard() {
    if (store.workspace) draft.value = cloneWorkspace(store.workspace)
    issues.value = []
    error.value = null
    attempted.value = false
  }

  useDraftGuard({
    label,
    isDirty: () => dirty.value,
    save,
    discard,
    snapshot: () => draft.value,
  })

  return { draft, dirty, issues, at, under, error, save, discard }
}
