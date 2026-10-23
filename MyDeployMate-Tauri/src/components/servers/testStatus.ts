import type { Component } from 'vue'
import { CircleAlert, CircleCheck, CircleSlash, LoaderCircle, ShieldAlert } from '@lucide/vue'

import type { Tone } from '@/components/ui/types'
import { translate, type MessageKey } from '@/i18n'
import type { ConnectError, ConnectionTest, ConnectStage } from '@/ipc'

export interface TestStatus {
  label: string
  tone: Tone
  icon?: Component
  spin?: boolean
}

/** IXD-15: name the actual result, never turn an old failure into "offline". */
export function testStatus(result: ConnectionTest | undefined, running: boolean): TestStatus {
  if (running)
    return {
      label: translate('connectStatus.testing'),
      tone: 'running',
      icon: LoaderCircle,
      spin: true,
    }
  if (!result) return { label: translate('connectStatus.untested'), tone: 'neutral' }
  const outcome = result.outcome
  switch (outcome.status) {
    case 'succeeded':
      return { label: translate('connectStatus.succeeded'), tone: 'success', icon: CircleCheck }
    case 'cancelled':
      return { label: translate('connectStatus.cancelled'), tone: 'neutral', icon: CircleSlash }
    case 'failed':
      return failureStatus(outcome.error)
  }
}

function failureStatus(error: ConnectError): TestStatus {
  const failure = (key: MessageKey): TestStatus => ({
    label: translate(key),
    tone: 'failure',
    icon: CircleAlert,
  })
  switch (error.kind) {
    case 'hostKeyChanged':
      return {
        label: translate('connectStatus.hostKeyChanged'),
        tone: 'failure',
        icon: ShieldAlert,
      }
    case 'hostKeyUnknown':
      return {
        label: translate('connectStatus.hostKeyUnknown'),
        tone: 'warning',
        icon: ShieldAlert,
      }
    case 'resolveFailed':
      return failure('connectStatus.resolveFailed')
    case 'connectionRefused':
      return failure('connectStatus.connectionRefused')
    case 'unreachable':
      return failure('connectStatus.unreachable')
    case 'timeout':
      return failure('connectStatus.timeout')
    case 'algorithmNegotiation':
      return failure('connectStatus.algorithmNegotiation')
    case 'authenticationFailed':
    case 'keyPassphraseWrong':
      return failure('connectStatus.authenticationFailed')
    case 'agentUnavailable':
      return failure('connectStatus.agentUnavailable')
    case 'agentNoIdentities':
      return failure('connectStatus.agentNoIdentities')
    case 'keyFileUnreadable':
    case 'keyFormatUnsupported':
      return failure('connectStatus.keyUnavailable')
    case 'passwordEnvMissing':
      return failure('connectStatus.passwordEnvMissing')
    case 'commandFailed':
      return failure('connectStatus.commandFailed')
    case 'disconnected':
      return failure('connectStatus.disconnected')
    case 'cancelled':
      return { label: translate('connectStatus.cancelled'), tone: 'neutral', icon: CircleSlash }
    case 'protocol':
      return failure('connectStatus.protocol')
  }
}

export function stageLabel(stage: ConnectStage): string {
  return translate(`connectStatus.stage.${stage}`)
}

/** What the test details offer as the next step (IXD-18). */
export type FixAction = 'editServer' | 'trustManagement' | null

export function fixAction(error: ConnectError): FixAction {
  switch (error.kind) {
    case 'hostKeyChanged':
      return 'trustManagement'
    case 'algorithmNegotiation':
    case 'authenticationFailed':
    case 'keyPassphraseWrong':
    case 'keyFileUnreadable':
    case 'keyFormatUnsupported':
    case 'passwordEnvMissing':
    case 'resolveFailed':
    case 'connectionRefused':
      return 'editServer'
    default:
      return null
  }
}

export function formatTime(ms: number): string {
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getMonth() + 1}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : translate('common.seconds', { n: (ms / 1000).toFixed(1) })
}
