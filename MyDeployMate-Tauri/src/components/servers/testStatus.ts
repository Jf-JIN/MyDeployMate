import type { Component } from 'vue'
import { CircleAlert, CircleCheck, CircleSlash, LoaderCircle, ShieldAlert } from '@lucide/vue'

import type { Tone } from '@/components/ui/types'
import type { ConnectError, ConnectionTest, ConnectStage } from '@/ipc'

export interface TestStatus {
  label: string
  tone: Tone
  icon?: Component
  spin?: boolean
}

/** IXD-15: name the actual result, never turn an old failure into "offline". */
export function testStatus(result: ConnectionTest | undefined, running: boolean): TestStatus {
  if (running) return { label: '测试中', tone: 'running', icon: LoaderCircle, spin: true }
  if (!result) return { label: '未测试', tone: 'neutral' }
  const outcome = result.outcome
  switch (outcome.status) {
    case 'succeeded':
      return { label: '连接正常', tone: 'success', icon: CircleCheck }
    case 'cancelled':
      return { label: '已取消', tone: 'neutral', icon: CircleSlash }
    case 'failed':
      return failureStatus(outcome.error)
  }
}

function failureStatus(error: ConnectError): TestStatus {
  const failure = (label: string): TestStatus => ({ label, tone: 'failure', icon: CircleAlert })
  switch (error.kind) {
    case 'hostKeyChanged':
      return { label: '身份不一致', tone: 'failure', icon: ShieldAlert }
    case 'hostKeyUnknown':
      return { label: '待核对身份', tone: 'warning', icon: ShieldAlert }
    case 'resolveFailed':
      return failure('解析失败')
    case 'connectionRefused':
      return failure('拒绝连接')
    case 'unreachable':
      return failure('网络不可达')
    case 'timeout':
      return failure('超时')
    case 'algorithmNegotiation':
      return failure('算法不兼容')
    case 'authenticationFailed':
    case 'keyPassphraseWrong':
      return failure('认证失败')
    case 'agentUnavailable':
      return failure('Agent 不可用')
    case 'agentNoIdentities':
      return failure('Agent 无密钥')
    case 'keyFileUnreadable':
    case 'keyFormatUnsupported':
      return failure('私钥不可用')
    case 'passwordEnvMissing':
      return failure('缺少密码变量')
    case 'commandFailed':
      return failure('命令失败')
    case 'disconnected':
      return failure('连接断开')
    case 'cancelled':
      return { label: '已取消', tone: 'neutral', icon: CircleSlash }
    case 'protocol':
      return failure('连接失败')
  }
}

const STAGES: Record<ConnectStage, string> = {
  prepare: '准备认证',
  resolve: '解析地址',
  connect: '建立连接',
  handshake: 'SSH 握手',
  hostKey: '核对服务器身份',
  authenticate: '登录认证',
  command: '探测命令',
}

export function stageLabel(stage: ConnectStage): string {
  return STAGES[stage]
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
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} 秒`
}
