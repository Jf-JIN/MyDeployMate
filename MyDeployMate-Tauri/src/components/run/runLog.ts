import type { LogEntry, LogSource } from '@/ipc'

/** One display line of a run log (IXD-27). */
export interface LogLine {
  /** Position in the whole log, from 0. */
  index: number
  task: number
  source: LogSource
  text: string
  atMs: number
  /** The 50 MiB marker: later output was not kept. */
  truncated: boolean
}

/**
 * Log entries cut into lines. Entries carry raw chunks, so a line can arrive in pieces and
 * pieces of stdout and stderr can interleave: each task and stream keeps its own open line.
 * Entries repeated after reopening a run are dropped by their number.
 */
export class RunLogLines {
  readonly lines: LogLine[] = []
  private lastN = -1
  /** Open (unterminated) line per task and stream, by line index. */
  private open = new Map<string, number>()

  /** Add entries in arrival order; returns whether anything changed. */
  add(entries: readonly LogEntry[]): boolean {
    let changed = false
    for (const entry of entries) {
      if (entry.n <= this.lastN) continue
      this.lastN = entry.n
      changed = true
      this.addEntry(entry)
    }
    return changed
  }

  private addEntry(entry: LogEntry) {
    const key = `${entry.task}:${entry.source}`
    const parts = entry.text.split('\n')
    // The text after the last newline stays open; an empty one means the entry ended a line.
    const rest = parts.pop() ?? ''
    parts.forEach((part, i) => {
      if (i === 0 && this.open.has(key)) {
        const line = this.lines[this.open.get(key) ?? 0]
        if (line) line.text = trimCr(line.text + part)
        this.open.delete(key)
      } else {
        this.push(entry, trimCr(part))
      }
    })
    if (rest) {
      const at = this.open.get(key)
      const line = at === undefined ? undefined : this.lines[at]
      if (line && parts.length === 0) {
        line.text += rest
      } else {
        this.open.set(key, this.push(entry, rest))
      }
    }
  }

  private push(entry: LogEntry, text: string): number {
    const index = this.lines.length
    this.lines.push({
      index,
      task: entry.task,
      source: entry.source,
      text,
      atMs: entry.atMs,
      truncated: !!entry.truncated,
    })
    return index
  }
}

function trimCr(text: string): string {
  return text.endsWith('\r') ? text.slice(0, -1) : text
}

/** Pieces of `text` with the case-insensitive matches of `query` marked, for highlighting. */
export function highlight(text: string, query: string): { text: string; hit: boolean }[] {
  if (!query) return [{ text, hit: false }]
  const lower = text.toLowerCase()
  const q = query.toLowerCase()
  const out: { text: string; hit: boolean }[] = []
  let from = 0
  for (let at = lower.indexOf(q); at !== -1; at = lower.indexOf(q, from)) {
    if (at > from) out.push({ text: text.slice(from, at), hit: false })
    out.push({ text: text.slice(at, at + q.length), hit: true })
    from = at + q.length
  }
  if (from < text.length || out.length === 0) out.push({ text: text.slice(from), hit: false })
  return out
}

/** HH:MM:SS.mmm in local time. */
export function clockTime(atMs: number): string {
  const d = new Date(atMs)
  const two = (n: number) => String(n).padStart(2, '0')
  return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}.${String(d.getMilliseconds()).padStart(3, '0')}`
}
