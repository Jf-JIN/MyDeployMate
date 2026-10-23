import { translate } from '@/i18n'
import type { ArtifactChange, Task, UploadDetail, UploadProgress } from '@/ipc'

/** Byte count as the backend writes it (`size_text`): 512 B, 3.4 KiB, 12.0 MiB. */
export function sizeText(bytes: number): string {
  const k = 1024
  if (bytes < k) return `${bytes} B`
  if (bytes < k * k) return `${(bytes / k).toFixed(1)} KiB`
  if (bytes < k * k * k) return `${(bytes / k / k).toFixed(1)} MiB`
  return `${(bytes / k / k / k).toFixed(1)} GiB`
}

/** "校验产物…" or "2/5 个文件 · 1.2 MiB / 4.0 MiB" (STEP-04, ARC-28). */
export function progressText(p: UploadProgress): string {
  if (p.phase === 'verifying') return translate('upload.verifying')
  return translate('upload.progress', {
    done: p.filesDone,
    total: p.filesTotal,
    sent: sizeText(p.bytesDone),
    size: sizeText(p.bytesTotal),
  })
}

/** Share of bytes sent, 0–1; files count when there are no bytes. */
export function progressRatio(p: UploadProgress): number {
  if (p.phase === 'verifying') return 0
  if (p.bytesTotal > 0) return Math.min(1, p.bytesDone / p.bytesTotal)
  return p.filesTotal > 0 ? p.filesDone / p.filesTotal : 1
}

export function uploadDetail(task: Task): UploadDetail | null {
  return task.detail?.type === 'upload' ? task.detail : null
}

export function changeLabel(change: ArtifactChange): string {
  return translate(`upload.change.${change.kind}`)
}
