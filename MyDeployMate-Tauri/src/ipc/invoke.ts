// Typed wrapper around Tauri `invoke`. Every command goes through `call`, so errors always
// arrive as `AppError` and command names and payloads are checked by the compiler.
import { invoke } from '@tauri-apps/api/core'

import type { AppError } from './bindings/AppError'
import type { AppInfo } from './bindings/AppInfo'
import type { BuiltinVariable } from './bindings/BuiltinVariable'
import type { Environment } from './bindings/Environment'
import type { EnvironmentId } from './bindings/EnvironmentId'
import type { KeyFormat } from './bindings/KeyFormat'
import type { LoadedWorkspace } from './bindings/LoadedWorkspace'
import type { Pipeline } from './bindings/Pipeline'
import type { PlanRequest } from './bindings/PlanRequest'
import type { Project } from './bindings/Project'
import type { ProjectId } from './bindings/ProjectId'
import type { Reference } from './bindings/Reference'
import type { ResolvedPlan } from './bindings/ResolvedPlan'
import type { Server } from './bindings/Server'
import type { ServerEdit } from './bindings/ServerEdit'
import type { ServerId } from './bindings/ServerId'
import type { ShellProgram } from './bindings/ShellProgram'
import type { Step } from './bindings/Step'
import type { StepKind } from './bindings/StepKind'
import type { ValidationReport } from './bindings/ValidationReport'
import type { Variable } from './bindings/Variable'
import type { VariableView } from './bindings/VariableView'
import type { Workspace } from './bindings/Workspace'

/** Command name → [arguments, result]. Keep in sync with `src-tauri/src/lib.rs`. */
export interface Commands {
  app_info: [Record<string, never>, AppInfo]
  load_workspace: [Record<string, never>, LoadedWorkspace]
  validate_workspace_draft: [{ draft: Workspace }, ValidationReport]
  save_workspace: [{ draft: Workspace }, Workspace]
  open_data_dir: [Record<string, never>, null]
  new_server: [{ environmentId: EnvironmentId }, Server]
  new_environment: [Record<string, never>, Environment]
  new_variable: [Record<string, never>, Variable]
  environment_references: [{ draft: Workspace; environmentId: EnvironmentId }, Reference[]]
  server_references: [{ draft: Workspace; serverId: ServerId }, Reference[]]
  put_server: [{ draft: Workspace; server: Server }, ServerEdit]
  remove_server: [{ draft: Workspace; serverId: ServerId }, ServerEdit]
  inspect_key_file: [{ path: string }, KeyFormat]
  env_var_present: [{ name: string }, boolean]
  new_project: [{ defaultEnvironmentId: EnvironmentId }, Project]
  copy_project: [{ draft: Workspace; projectId: ProjectId; name: string }, Project]
  variable_views: [
    { draft: Workspace; projectId: ProjectId; environmentId: EnvironmentId },
    VariableView[],
  ]
  local_dir_exists: [{ path: string }, boolean]
  new_pipeline: [{ name: string }, Pipeline]
  copy_pipeline: [{ pipeline: Pipeline; name: string }, Pipeline]
  new_step: [{ kind: StepKind }, Step]
  copy_step: [{ step: Step }, Step]
  builtin_variables: [Record<string, never>, BuiltinVariable[]]
  wsl_distros: [Record<string, never>, string[]]
  shell_programs: [Record<string, never>, ShellProgram[]]
  preview_run: [{ draft: Workspace; request: PlanRequest }, ResolvedPlan]
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
