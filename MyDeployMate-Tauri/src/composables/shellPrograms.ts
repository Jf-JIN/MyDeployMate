// Which executable each local shell runs on this computer (STEP-02, ARC-17). Read once per app
// start; the step editor and the preview show it so the user sees what will actually run.
import { ref } from 'vue'

import { translate } from '@/i18n'
import type { Shell } from '@/ipc/bindings/Shell'
import type { ShellProgram } from '@/ipc/bindings/ShellProgram'
import { call } from '@/ipc/invoke'

export function shellLabel(shell: Shell): string {
  switch (shell) {
    case 'powershell':
      return 'Windows PowerShell'
    case 'pwsh':
      return translate('shell.pwsh')
    case 'cmd':
      return 'cmd'
    case 'wsl':
      return 'WSL'
  }
}

const PROGRAM_NAME: Record<Shell, string> = {
  powershell: 'powershell.exe',
  pwsh: 'pwsh.exe',
  cmd: 'cmd.exe',
  wsl: 'wsl.exe',
}

const programs = ref<ShellProgram[] | null>(null)
let loading: Promise<void> | null = null

export function useShellPrograms() {
  loading ??= call('shell_programs')
    .then((list) => {
      programs.value = list
    })
    .catch(() => {
      programs.value = []
    })

  /** Path of the shell's executable; `undefined` until loaded, `null` when not found. */
  function programPath(shell: Shell): string | null | undefined {
    if (programs.value === null) return undefined
    return programs.value.find((p) => p.shell === shell)?.path ?? null
  }

  /** One line for hints: the resolved path, or that the program is missing. */
  function programNote(shell: Shell): string | undefined {
    const path = programPath(shell)
    if (path === undefined) return undefined
    return path === null
      ? translate('shell.missing', { program: PROGRAM_NAME[shell] })
      : translate('shell.willRun', { path })
  }

  return { programPath, programNote }
}
