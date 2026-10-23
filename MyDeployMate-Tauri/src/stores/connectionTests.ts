import { Channel } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { errorText } from '@/i18n/message'
import {
  call,
  type ConnectionTest,
  type CredentialPrompt,
  type HostEndpoint,
  type HostKey,
  type HostTrustPrompt,
  type TestEvent,
} from '@/ipc'

/** A D02 request from a running test: first connection to a server. */
export type TrustRequest = HostTrustPrompt & { requestId: number }

/** A D02 notice after a test was blocked because the key changed. */
export interface KeyChange {
  serverId: string
  serverName: string
  user: string
  endpoint: HostEndpoint
  known: HostKey[]
  presented: HostKey
}

/** A D04 request from a running test, with the server it belongs to. */
export interface CredentialRequest {
  serverId: string
  requestId: number
  prompt: CredentialPrompt
}

export interface Running {
  operationId: string
  startedAtMs: number
}

export interface Batch {
  total: number
  done: number
  cancelled: boolean
}

let nextOperation = 1

/**
 * Connection tests (IXD-18). Single tests run side by side, one per server; a batch runs its
 * servers one after another. Results live in the backend, which drops a result once the
 * server's connection fields change (SRV-05), so `load` after every save keeps the list honest.
 */
export const useConnectionTestStore = defineStore('connectionTests', () => {
  const results = ref<Record<string, ConnectionTest>>({})
  const running = ref<Record<string, Running>>({})
  const batch = ref<Batch | null>(null)
  const trustRequests = ref<TrustRequest[]>([])
  const credentialRequests = ref<CredentialRequest[]>([])
  const keyChange = ref<KeyChange | null>(null)
  const error = ref<string | null>(null)

  async function load() {
    try {
      const list = await call('connection_tests')
      results.value = Object.fromEntries(list.map((t) => [t.serverId, t]))
    } catch (e) {
      error.value = errorText(e)
    }
  }

  async function test(serverId: string, names: { name: string; user: string }) {
    if (running.value[serverId]) return null
    const operationId = `test-${Date.now()}-${nextOperation++}`
    running.value = { ...running.value, [serverId]: { operationId, startedAtMs: Date.now() } }
    const channel = new Channel<TestEvent>()
    channel.onmessage = ({ requestId, prompt }) => {
      if (prompt.type === 'hostTrust') {
        trustRequests.value = [...trustRequests.value, { ...prompt, requestId }]
      } else {
        credentialRequests.value = [...credentialRequests.value, { serverId, requestId, prompt }]
      }
    }
    try {
      const result = await call('test_connection', { serverId, operationId, onEvent: channel })
      results.value = { ...results.value, [serverId]: result }
      const outcome = result.outcome
      if (outcome.status === 'failed' && outcome.error.kind === 'hostKeyChanged') {
        keyChange.value = {
          serverId,
          serverName: names.name,
          user: names.user,
          endpoint: outcome.error.endpoint,
          known: outcome.error.known,
          presented: outcome.error.presented,
        }
      }
      return result
    } catch (e) {
      error.value = errorText(e)
      return null
    } finally {
      const rest = { ...running.value }
      delete rest[serverId]
      running.value = rest
      trustRequests.value = trustRequests.value.filter((r) => r.serverId !== serverId)
      credentialRequests.value = credentialRequests.value.filter((r) => r.serverId !== serverId)
    }
  }

  async function cancel(serverId: string) {
    const op = running.value[serverId]
    if (op) await call('cancel_connection_test', { operationId: op.operationId })
  }

  /** Test servers one after another; `cancelBatch` stops the current one and the rest. */
  async function testAll(servers: { id: string; name: string; user: string }[]) {
    if (batch.value) return
    batch.value = { total: servers.length, done: 0, cancelled: false }
    try {
      for (const s of servers) {
        if (batch.value.cancelled) break
        await test(s.id, s)
        batch.value.done++
      }
    } finally {
      batch.value = null
    }
  }

  async function cancelBatch() {
    if (!batch.value) return
    batch.value.cancelled = true
    await Promise.all(Object.keys(running.value).map((id) => cancel(id)))
  }

  /** Answer the D02 request at the head of the queue. */
  async function answerTrust(trust: boolean) {
    const request = trustRequests.value[0]
    if (!request) return
    trustRequests.value = trustRequests.value.slice(1)
    try {
      await call('answer_host_trust', { requestId: request.requestId, trust })
    } catch (e) {
      error.value = errorText(e)
    }
  }

  /**
   * Answer the D04 request at the head of the queue. `null` cancels that test only; a batch
   * goes on with the next server (IXD-18, IXD-32).
   */
  async function answerCredential(secret: string | null, remember: boolean) {
    const request = credentialRequests.value[0]
    if (!request) return
    credentialRequests.value = credentialRequests.value.slice(1)
    try {
      await call('submit_secret', { requestId: request.requestId, secret, remember })
    } catch (e) {
      error.value = errorText(e)
    }
  }

  const pendingTrust = computed(() => trustRequests.value[0] ?? null)
  const pendingCredential = computed(() => credentialRequests.value[0] ?? null)

  return {
    results,
    running,
    batch,
    keyChange,
    error,
    pendingTrust,
    pendingCredential,
    load,
    test,
    cancel,
    testAll,
    cancelBatch,
    answerTrust,
    answerCredential,
  }
})
