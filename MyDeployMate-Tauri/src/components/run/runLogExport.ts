import { translate } from '@/i18n'
import type { RunDetail } from '@/stores/run'

import { dateTime } from './runState'

// IXD-27 / OBS-02: the exported log keeps time and context on every line. Lines past the body
// limit that were only shown live are left out: the export holds what was saved.

/** `account-api_构建并部署_20261024-053009.log`; characters Windows refuses become `_`. */
export function exportFileName(detail: RunDetail): string {
  const r = detail.run
  const stamp = dateTime(r.createdAtMs).replace(/[-:]/g, '').replace(' ', '-')
  return `${r.projectName}_${r.pipelineName}_${stamp}.log`.replace(/[\\/:*?"<>|\s]+/g, '_')
}

export function formatRunLog(detail: RunDetail): string {
  const r = detail.run
  const out = [
    translate('log.export.header', {
      project: r.projectName,
      pipeline: r.pipelineName,
      environment: r.environmentName,
      version: r.version,
    }),
    translate('log.export.run', { id: r.id, time: dateTime(r.createdAtMs) }),
    translate('log.export.notice'),
    '',
  ]
  const local = translate('common.local')
  const system = translate('log.system')
  for (const line of detail.lines.lines) {
    if (line.live) continue
    const task = detail.plan.tasks[line.task]
    const where = task?.server?.name ?? local
    const step = task ? `#${task.seq + 1} ${task.stepName}` : `#${line.task + 1}`
    const source = line.source === 'system' ? system : line.source
    if (line.truncated) out.push(translate('log.export.truncated'))
    out.push(`${dateTime(line.atMs)} [${step} @ ${where}] [${source}] ${line.text}`)
  }
  return out.join('\r\n') + '\r\n'
}
