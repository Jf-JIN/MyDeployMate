export interface SelectOption<V> {
  value: V
  label: string
  /** Shown next to the label; the option cannot be picked (UI-18). */
  disabledReason?: string
}

export type Tone = 'neutral' | 'success' | 'failure' | 'warning' | 'running' | 'unknown'

/** An entry of `UiMenu`: an action, or a heading that starts a group. */
export interface MenuEntry {
  key: string
  label: string
  kind?: 'item' | 'heading'
  icon?: import('vue').Component
  /** Secondary text on the right, e.g. a variable's origin. */
  detail?: string
  danger?: boolean
  /** Draw a separator before this item, e.g. to set dangerous actions apart. */
  separator?: boolean
  disabledReason?: string | null
}
