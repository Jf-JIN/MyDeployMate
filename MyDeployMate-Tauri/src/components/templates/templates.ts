import { translate, type MessageKey } from '@/i18n'
import {
  call,
  type Pipeline,
  type PipelineTemplate,
  type Project,
  type Step,
  type TemplateId,
  type Workspace,
} from '@/ipc'

// P09 templates (PRJ-03, IXD-35). Rust gives the steps with keys for names and prompts; the
// copy made here carries the operator's language and is an independent, editable pipeline.

export const templateName = (id: TemplateId) => translate(`templates.${id}.name` as MessageKey)
export const templatePurpose = (id: TemplateId) =>
  translate(`templates.${id}.purpose` as MessageKey)

/** A deep copy that also works on reactive data, which `structuredClone` refuses. */
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T

/** A step name or prompt key resolved in the current language. */
function stepText(id: TemplateId, key: string): string {
  return translate(`templates.${id}.steps.${key}` as MessageKey)
}

export function templateSteps(template: PipelineTemplate): Step[] {
  const id = template.id
  return template.pipeline.steps.map((step) => {
    const copy: Step = clone(step)
    copy.name = stepText(id, step.name)
    if (copy.spec.type === 'confirm') copy.spec.prompt = stepText(id, copy.spec.prompt)
    return copy
  })
}

export function templatePipeline(template: PipelineTemplate, name: string): Pipeline {
  return { ...clone(template.pipeline), name, steps: templateSteps(template) }
}

/** Template variables that neither the project nor the global scope defines. */
export function missingVariables(
  template: PipelineTemplate,
  project: Project,
  workspace: Workspace,
): string[] {
  const known = new Set([...project.variables, ...workspace.globalVariables].map((v) => v.name))
  return template.variables.filter((v) => !known.has(v.name)).map((v) => v.name)
}

/** Add the missing template variables to `project` with their example values. */
export async function addTemplateVariables(
  template: PipelineTemplate,
  project: Project,
  workspace: Workspace,
): Promise<string[]> {
  const missing = missingVariables(template, project, workspace)
  for (const name of missing) {
    const variable = await call('new_variable')
    const value = template.variables.find((v) => v.name === name)?.value ?? ''
    project.variables.push({ ...variable, name, value: { kind: 'literal', value } })
  }
  return missing
}

/** `name`, or `name 2`, `name 3`… when the project already has a pipeline called that. */
export function uniquePipelineName(project: Project | undefined, name: string): string {
  const taken = new Set(project?.pipelines.map((p) => p.name) ?? [])
  if (!taken.has(name)) return name
  let n = 2
  while (taken.has(`${name} ${n}`)) n++
  return `${name} ${n}`
}
