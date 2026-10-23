import type { Label, Message } from '@/ipc'
import { toAppError } from '@/ipc'

import { translate, type MessageKey, type MessageParams } from './index'

/** Every message and label code the backend can send has a `backend.*` entry (ARC-38). */
type BackendKey = `backend.${Message['code'] | Label['code']}`
const everyCodeHasText: BackendKey extends MessageKey ? true : never = true
void everyCodeHasText

/**
 * The text of a backend message in the current language. Nested messages and labels are
 * rendered first; other parameters are raw values such as names and paths.
 */
export function messageText(message: Message | Label): string {
  const params: MessageParams = {}
  if ('params' in message) {
    for (const [name, value] of Object.entries(message.params)) {
      params[name] = typeof value === 'object' ? messageText(value as Message) : value
    }
  }
  return translate(`backend.${message.code}` as MessageKey, params)
}

/** What a failed command reports, for a notice or toast. */
export function errorText(error: unknown): string {
  return messageText(toAppError(error).message)
}

/** The text of a validation issue, or null when there is none (for `UiField`'s `error`). */
export function issueText(issue: { message: Message } | null | undefined): string | null {
  return issue ? messageText(issue.message) : null
}
