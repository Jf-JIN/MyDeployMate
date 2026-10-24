import { describe, expect, it } from 'vitest'

import type { PipelineTemplate, Project, Workspace } from '@/ipc'

import { missingVariables, templatePipeline, uniquePipelineName } from './templates'

const template = {
  id: 'localBuildUpload',
  pipeline: {
    id: 'p',
    name: '',
    executionOrder: 'stepFirst',
    failurePolicy: 'stop',
    steps: [
      { id: 's1', name: 'build', spec: { type: 'local', command: 'npm run build' } },
      { id: 's2', name: 'confirm', spec: { type: 'confirm', prompt: 'prompt' } },
    ],
  },
  variables: [
    { name: 'deploy_dir', value: '/opt/apps/${project.name}' },
    { name: 'port', value: '8080' },
  ],
  needsRepoAccess: false,
} as unknown as PipelineTemplate

const project = {
  pipelines: [{ name: '发布' }, { name: '发布 2' }],
  variables: [{ name: 'port' }],
} as unknown as Project

describe('templates', () => {
  it('names steps and prompts in the current language and keeps commands', () => {
    const p = templatePipeline(template, '新流程')
    expect(p.name).toBe('新流程')
    expect(p.steps.map((s) => s.name)).toEqual(['构建产物', '确认发布结果'])
    expect(p.steps[1]!.spec).toMatchObject({ prompt: '确认关键接口和业务指标正常后继续。' })
    expect(p.steps[0]!.spec).toMatchObject({ command: 'npm run build' })
    // The template itself is untouched.
    expect(template.pipeline.steps[0]!.name).toBe('build')
  })

  it('adds only variables the project and global scope lack', () => {
    const ws = { globalVariables: [] } as unknown as Workspace
    expect(missingVariables(template, project, ws)).toEqual(['deploy_dir'])
    const withGlobal = { globalVariables: [{ name: 'deploy_dir' }] } as unknown as Workspace
    expect(missingVariables(template, project, withGlobal)).toEqual([])
  })

  it('suggests a pipeline name the project does not use yet', () => {
    expect(uniquePipelineName(project, '部署')).toBe('部署')
    expect(uniquePipelineName(project, '发布')).toBe('发布 3')
  })
})
