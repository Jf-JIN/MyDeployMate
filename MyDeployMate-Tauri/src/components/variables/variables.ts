import { translate } from '@/i18n'
import type { Project, Variable, VariableOrigin, VariableOverride, VariableValue } from '@/ipc'

/** The override of `name` in an environment, if set (IXD-13: unset differs from empty). */
export function findOverride(
  project: Project,
  environmentId: string,
  name: string,
): VariableOverride | undefined {
  return project.overrides.find((o) => o.environmentId === environmentId && o.name === name)
}

/** A starting override value; sensitive variables never start as plain text (ARC-10). */
export function blankValue(sensitive: boolean): VariableValue {
  return sensitive ? { kind: 'envRef', name: '' } : { kind: 'literal', value: '' }
}

export function setOverride(
  project: Project,
  environmentId: string,
  name: string,
  value: VariableValue,
) {
  const existing = findOverride(project, environmentId, name)
  if (existing) existing.value = value
  else project.overrides.push({ environmentId, name, value })
}

/** Remove an override so the variable inherits again; the variable itself stays. */
export function clearOverride(project: Project, environmentId: string, name: string) {
  project.overrides = project.overrides.filter(
    (o) => !(o.environmentId === environmentId && o.name === name),
  )
}

/**
 * Overrides are keyed by name. Renaming a project variable carries its overrides along, unless
 * a global variable of the old name still owns them.
 */
export function renameVariable(project: Project, globals: Variable[], from: string, to: string) {
  if (from === to || globals.some((g) => g.name === from)) return
  for (const o of project.overrides) if (o.name === from) o.name = to
}

/** Delete a project variable and the overrides only it used. */
export function removeVariable(project: Project, globals: Variable[], id: string) {
  const variable = project.variables.find((v) => v.id === id)
  if (!variable) return
  project.variables = project.variables.filter((v) => v.id !== id)
  if (!globals.some((g) => g.name === variable.name)) {
    project.overrides = project.overrides.filter((o) => o.name !== variable.name)
  }
}

/** Plain-language source for the P04 source column. */
export function describeSource(
  origin: VariableOrigin,
  value: VariableValue | undefined,
  environment: string,
): string {
  const where = {
    global: () => translate('variables.source.global'),
    project: () => translate('variables.source.project'),
    environmentOverride: () => translate('variables.source.environmentOverride', { environment }),
    run: () => translate('variables.source.run'),
  }[origin]()
  if (value?.kind === 'envRef') {
    const name = value.name || translate('variables.source.unnamed')
    return translate('variables.source.envRef', { where, name })
  }
  if (value?.kind === 'secretRef') return translate('variables.source.secretRef', { where })
  return where
}
