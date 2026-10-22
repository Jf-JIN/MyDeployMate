export interface SelectOption<V> {
  value: V
  label: string
  /** Shown next to the label; the option cannot be picked (UI-18). */
  disabledReason?: string
}

export type Tone = 'neutral' | 'success' | 'failure' | 'warning' | 'running' | 'unknown'
