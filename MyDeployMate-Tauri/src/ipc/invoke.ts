// Typed wrapper around Tauri `invoke`. Every command goes through `call`, so errors always
// arrive as `AppError` and command names and payloads are checked by the compiler.
import { type Channel, invoke } from '@tauri-apps/api/core'

import type { AppError } from './bindings/AppError'
import type { AppInfo } from './bindings/AppInfo'
import type { BuiltinVariable } from './bindings/BuiltinVariable'
import type { CredentialRef } from './bindings/CredentialRef'
import type { ConnectionTest } from './bindings/ConnectionTest'
import type { Environment } from './bindings/Environment'
import type { EnvironmentId } from './bindings/EnvironmentId'
import type { HostEndpoint } from './bindings/HostEndpoint'
import type { HostKey } from './bindings/HostKey'
import type { ImportApplied } from './bindings/ImportApplied'
import type { ImportPreview } from './bindings/ImportPreview'
import type { KeyFormat } from './bindings/KeyFormat'
import type { LoadedWorkspace } from './bindings/LoadedWorkspace'
import type { Message } from './bindings/Message'
import type { Pipeline } from './bindings/Pipeline'
import type { PipelineTemplate } from './bindings/PipelineTemplate'
import type { PlanRequest } from './bindings/PlanRequest'
import type { Project } from './bindings/Project'
import type { ProjectId } from './bindings/ProjectId'
import type { Reference } from './bindings/Reference'
import type { ResolvedPlan } from './bindings/ResolvedPlan'
import type { Run } from './bindings/Run'
import type { RunChoice } from './bindings/RunChoice'
import type { RunFilter } from './bindings/RunFilter'
import type { SavedCredential } from './bindings/SavedCredential'
import type { RunId } from './bindings/RunId'
import type { RunSnapshot } from './bindings/RunSnapshot'
import type { RunSummary } from './bindings/RunSummary'
import type { RunUpdate } from './bindings/RunUpdate'
import type { Server } from './bindings/Server'
import type { ServerEdit } from './bindings/ServerEdit'
import type { ServerId } from './bindings/ServerId'
import type { ShellProgram } from './bindings/ShellProgram'
import type { StartRunRequest } from './bindings/StartRunRequest'
import type { Step } from './bindings/Step'
import type { StepKind } from './bindings/StepKind'
import type { TestEvent } from './bindings/TestEvent'
import type { TrustedHost } from './bindings/TrustedHost'
import type { ValidationReport } from './bindings/ValidationReport'
import type { Variable } from './bindings/Variable'
import type { VariableId } from './bindings/VariableId'
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
  new_project: [{ defaultEnvironmentId: EnvironmentId; pipelineName: string }, Project]
  copy_project: [{ draft: Workspace; projectId: ProjectId; name: string }, Project]
  variable_views: [
    { draft: Workspace; projectId: ProjectId; environmentId: EnvironmentId },
    VariableView[],
  ]
  local_dir_exists: [{ path: string }, boolean]
  new_pipeline: [{ name: string }, Pipeline]
  copy_pipeline: [{ pipeline: Pipeline; name: string }, Pipeline]
  new_step: [{ kind: StepKind; name: string }, Step]
  copy_step: [{ step: Step; name: string }, Step]
  builtin_variables: [Record<string, never>, BuiltinVariable[]]
  pipeline_templates: [Record<string, never>, PipelineTemplate[]]
  wsl_distros: [Record<string, never>, string[]]
  shell_programs: [Record<string, never>, ShellProgram[]]
  preview_run: [{ draft: Workspace; request: PlanRequest }, ResolvedPlan]
  list_runs: [{ filter: RunFilter }, RunSummary[]]
  run_choices: [Record<string, never>, RunChoice[]]
  latest_runs: [Record<string, never>, Run[]]
  delete_run: [{ runId: RunId }, null]
  expired_run_count: [{ days: number }, number]
  purge_history: [Record<string, never>, number]
  plan_changes: [{ runId: RunId; plan: ResolvedPlan }, Message[]]
  write_export: [{ path: string; contents: string }, null]
  export_workspace: [{ path: string; includeLocalPaths: boolean }, null]
  import_preview: [{ path: string }, ImportPreview]
  import_apply: [{ token: number }, ImportApplied]
  start_run: [{ start: StartRunRequest }, RunId]
  open_run: [{ runId: RunId; onUpdate: Channel<RunUpdate> }, RunSnapshot]
  active_run: [Record<string, never>, RunId | null]
  cancel_run: [{ runId: RunId }, null]
  answer_confirmation: [{ runId: RunId; seq: number; proceed: boolean }, null]
  answer_run_secret: [
    { runId: RunId; requestId: number; secret: string | null; remember: boolean },
    null,
  ]
  answer_run_trust: [{ runId: RunId; requestId: number; trust: boolean }, null]
  active_run_servers: [Record<string, never>, ServerId[]]
  cancel_and_quit: [Record<string, never>, null]
  connection_tests: [Record<string, never>, ConnectionTest[]]
  test_connection: [
    { serverId: ServerId; operationId: string; onEvent: Channel<TestEvent> },
    ConnectionTest,
  ]
  cancel_connection_test: [{ operationId: string }, null]
  answer_host_trust: [{ requestId: number; trust: boolean }, null]
  trusted_hosts: [Record<string, never>, TrustedHost[]]
  remove_trusted_host: [{ endpoint: HostEndpoint }, null]
  fetch_host_key: [{ endpoint: HostEndpoint; timeoutSec: number }, HostKey]
  replace_trusted_host: [{ endpoint: HostEndpoint; key: HostKey }, null]
  submit_secret: [{ requestId: number; secret: string | null; remember: boolean }, null]
  saved_credentials: [Record<string, never>, SavedCredential[]]
  forget_credential: [{ credential: CredentialRef }, null]
  set_variable_secret: [
    { variableId: VariableId; environmentId: EnvironmentId | null; value: string },
    null,
  ]
  variable_secret_present: [
    { variableId: VariableId; environmentId: EnvironmentId | null },
    boolean,
  ]
}

export type CommandName = keyof Commands

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'kind' in value &&
    'message' in value &&
    typeof (value as { message: unknown }).message === 'object'
  )
}

/** Turn anything thrown by `invoke` into an `AppError`; other errors keep their own text. */
export function toAppError(error: unknown): AppError {
  if (isAppError(error)) return error
  const text = error instanceof Error ? error.message : String(error)
  return { kind: 'internal', message: { code: 'text', params: { text } }, validation: null }
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
