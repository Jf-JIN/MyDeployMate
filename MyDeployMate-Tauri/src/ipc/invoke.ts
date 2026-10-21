// Typed wrapper around Tauri `invoke`. Every command goes through `call`, so errors always
// arrive as `AppError` and command names and payloads are checked by the compiler.
import { invoke } from '@tauri-apps/api/core'

import type { AppError } from './bindings/AppError'
import type { AppInfo } from './bindings/AppInfo'
import type { LoadedWorkspace } from './bindings/LoadedWorkspace'
import type { ValidationReport } from './bindings/ValidationReport'
import type { Workspace } from './bindings/Workspace'

/** Command name → [arguments, result]. Keep in sync with `src-tauri/src/lib.rs`. */
export interface Commands {
  app_info: [Record<string, never>, AppInfo]
  load_workspace: [Record<string, never>, LoadedWorkspace]
  validate_workspace_draft: [{ draft: Workspace }, ValidationReport]
  save_workspace: [{ draft: Workspace }, Workspace]
}

export type CommandName = keyof Commands

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'kind' in value &&
    'message' in value &&
    typeof (value as { message: unknown }).message === 'string'
  )
}

/** Turn anything thrown by `invoke` into an `AppError`. */
export function toAppError(error: unknown): AppError {
  if (isAppError(error)) return error
  const message = error instanceof Error ? error.message : String(error)
  return { kind: 'internal', message, validation: null }
}

export async function call<K extends CommandName>(
  command: K,
  ...args: Commands[K][0] extends Record<string, never> ? [] : [Commands[K][0]]
): Promise<Commands[K][1]> {
  try {
    return await invoke<Commands[K][1]>(command, args[0])
  } catch (error) {
    throw toAppError(error)
  }
}
