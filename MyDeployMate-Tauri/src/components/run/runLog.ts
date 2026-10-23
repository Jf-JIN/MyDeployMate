import { messageText } from '@/i18n/message'
import type { LogEntry, LogSource } from '@/ipc'

/** Lines of output past the 50 MiB limit kept on screen; older ones are dropped (NFR-02). */
export const LIVE_LINES = 5000

/** One display line of a run log (IXD-27). */
export interface LogLine {
  /** Unique within the log, in order. */
  index: number
  task: number
  source: LogSource
  text: string
  atMs: number
  /** The 50 MiB marker: later output was not kept in full. */
  truncated: boolean
  /** Past the limit: shown live, not saved. */
  live: boolean
}

/**
 * Log entries cut into lines. Entries carry raw chunks, so a line can arrive in pieces and
 * pieces of stdout and stderr can interleave: each task and stream keeps its own open line.
 * Entries repeated after reopening a run are dropped by their number. Past the body limit
 * only the newest [`LIVE_LINES`] live lines stay.
 */
export class RunLogLines {
  readonly lines: LogLine[] = []
  /** Live lines dropped to keep memory bounded. */
  trimmed = 0
  private lastN = -1
  private nextIndex = 0
  private live = 0
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
    if (this.live > 2 * LIVE_LINES) this.trim()
    return changed
  }

  private addEntry(entry: LogEntry) {
    // Output after the marker is either live or a saved tail: neither continues an earlier line.
    if (entry.truncated) this.open.clear()
    const key = `${entry.task}:${entry.source}`
    // An app note is rendered once, in the language current when it arrives.
    const text = entry.message ? messageText(entry.message) + entry.text : entry.text
    const parts = text.split('\n')
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
    const at = this.lines.length
    this.lines.push({
      index: this.nextIndex++,
      task: entry.task,
      source: entry.source,
      text,
      atMs: entry.atMs,
      truncated: !!entry.truncated,
      live: !!entry.live,
    })
    if (entry.live) this.live++
    return at
  }

  /** Drop the oldest live lines down to [`LIVE_LINES`], in place. */
  private trim() {
    let drop = this.live - LIVE_LINES
    const openAt = new Map<number, string>()
    for (const [key, at] of this.open) openAt.set(at, key)
    this.open.clear()
    let w = 0
    for (let r = 0; r < this.lines.length; r++) {
      const line = this.lines[r] as LogLine
      if (drop > 0 && line.live) {
        drop--
        continue
      }
      const key = openAt.get(r)
      if (key !== undefined) this.open.set(key, w)
      this.lines[w++] = line
    }
    this.trimmed += this.lines.length - w
    this.live -= this.lines.length - w
    this.lines.length = w
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
