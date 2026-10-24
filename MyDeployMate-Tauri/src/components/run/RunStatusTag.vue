<script setup lang="ts">
import {
  CircleCheck,
  CircleDashed,
  CircleHelp,
  CircleSlash,
  CircleX,
  KeyRound,
  LoaderCircle,
  PauseCircle,
} from '@lucide/vue'
import type { Component } from 'vue'

import UiTag from '@/components/ui/UiTag.vue'
import type { RunStatus } from '@/ipc'

import { RUN_STATUS } from './runState'

// A run's result in lists (P01, P08): text, tone and an icon, never colour alone (NFR-02).
defineProps<{ status: RunStatus }>()

const ICON: Record<RunStatus, Component> = {
  queued: CircleDashed,
  running: LoaderCircle,
  waitingConfirmation: PauseCircle,
  waitingCredential: KeyRound,
  cancelling: LoaderCircle,
  succeeded: CircleCheck,
  failed: CircleX,
  cancelled: CircleSlash,
  interrupted: CircleHelp,
}
</script>

<template>
  <UiTag
    :tone="RUN_STATUS[status].tone"
    :icon="ICON[status]"
    :spin="status === 'running' || status === 'cancelling'"
  >
    {{ RUN_STATUS[status].label }}
  </UiTag>
</template>
