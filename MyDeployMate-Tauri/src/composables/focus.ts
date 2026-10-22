// Focus helpers for dialogs (IXD-03, UI-14, UI-30): trap Tab inside the top layer, let only the
// top layer react to Esc, and hand focus back to the trigger when a dialog closes.

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

export function focusableIn(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => !el.closest('[inert]'),
  )
}

/** Keep Tab and Shift+Tab inside `root`. Returns true when the event was handled. */
export function trapTab(root: HTMLElement, event: KeyboardEvent): boolean {
  if (event.key !== 'Tab') return false
  const items = focusableIn(root)
  const first = items[0]
  const last = items[items.length - 1]
  if (!first || !last) {
    event.preventDefault()
    root.focus()
    return true
  }
  const active = document.activeElement
  if (event.shiftKey && (active === first || !root.contains(active))) {
    event.preventDefault()
    last.focus()
    return true
  }
  if (!event.shiftKey && (active === last || !root.contains(active))) {
    event.preventDefault()
    first.focus()
    return true
  }
  return false
}

/** Focus `[autofocus]`, else the first focusable field in the body, else the root itself. */
export function focusInitial(root: HTMLElement, body?: HTMLElement | null) {
  const target =
    root.querySelector<HTMLElement>('[autofocus]') ??
    (body ? focusableIn(body)[0] : undefined) ??
    focusableIn(root)[0] ??
    root
  target.focus()
}

// Open layers, bottom to top. UI-14 allows one main dialog plus one confirm layer.
const layers: symbol[] = []

export function pushLayer(): symbol {
  const id = Symbol('layer')
  layers.push(id)
  return id
}

export function popLayer(id: symbol) {
  const index = layers.indexOf(id)
  if (index >= 0) layers.splice(index, 1)
}

export function isTopLayer(id: symbol): boolean {
  return layers[layers.length - 1] === id
}

export function openLayerCount(): number {
  return layers.length
}
