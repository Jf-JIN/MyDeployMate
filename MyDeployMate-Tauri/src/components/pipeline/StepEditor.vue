<script setup lang="ts">
import { FolderOpen } from '@lucide/vue'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, ref, useId, watch } from 'vue'

import { shellLabel, useShellPrograms } from '@/composables/shellPrograms'
import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiNumberInput from '@/components/ui/UiNumberInput.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useT } from '@/i18n'
import {
  call,
  type LocalSpec,
  type Pipeline,
  type RemoteCommand,
  type RemoteSpec,
  type Server,
  type Step,
  type StepFailurePolicy,
  type StepPrivilege,
  type UploadSpec,
} from '@/ipc'

import { effectivePrivilege } from './pipeline'
import VariableMenu from './VariableMenu.vue'

const t = useT()

// IXD-10 / IXD-11: the parameter panel of one step. Common fields stay in the same place for
// every kind; a step's kind never changes here (add a new step instead).
/** The draft step; edited in place, as the page draft owns it. */
const step = defineModel<Step>('step', { required: true })
const props = defineProps<{
  pipeline: Pipeline
  /** Bound servers of the current environment, for the per-target privilege list. */
  targets: Server[]
  /** Issue message for a field of this step, e.g. `spec.command`. */
  errorAt: (field: string) => string | null
}>()

const uid = useId()
const fid = (name: string) => `${uid}-${name}`

const kindLabel = computed(() => t(`steps.kind.${step.value.spec.type}`))

// ---- Common ----------------------------------------------------------------------------------

const failureOptions = computed(() => {
  const local = step.value.spec.type === 'local'
  const stepFirst = props.pipeline.executionOrder !== 'serverFirst'
  return [
    {
      value: 'inherit' as StepFailurePolicy,
      label: t('stepEditor.inherit', {
        policy: t(`pipelines.settings.${props.pipeline.failurePolicy}`),
      }),
    },
    { value: 'stop' as StepFailurePolicy, label: t('pipelines.settings.stop') },
    { value: 'continue' as StepFailurePolicy, label: t('pipelines.settings.continue') },
    {
      value: 'skipServer' as StepFailurePolicy,
      label: t('pipelines.settings.skipServer'),
      disabledReason: local
        ? t('variableMenu.noServer')
        : stepFirst
          ? t('pipelines.settings.serverFirstOnly')
          : undefined,
    },
  ]
})

const failureHint = computed(() => {
  const policy =
    step.value.failurePolicy === 'inherit' ? props.pipeline.failurePolicy : step.value.failurePolicy
  return t(`stepEditor.failureHint.${policy}`)
})

// ---- Local ---------------------------------------------------------------------------------

const local = computed(() =>
  step.value.spec.type === 'local' ? (step.value.spec as LocalSpec) : null,
)
const shellOptions = computed(() =>
  (['powershell', 'pwsh', 'cmd', 'wsl'] as const).map((value) => ({
    value,
    label: shellLabel(value),
  })),
)
const { programNote } = useShellPrograms()
const encodingOptions = computed(() => [
  { value: 'auto' as const, label: t('stepEditor.encodingAuto') },
  { value: 'utf8' as const, label: 'UTF-8' },
  { value: 'gbk' as const, label: 'GBK' },
])
const distros = ref<string[] | null>(null)
onMounted(async () => {
  distros.value = await call('wsl_distros').catch(() => [])
})
const distroOptions = computed(() => (distros.value ?? []).map((d) => ({ value: d, label: d })))

/** Optional text fields stored as `null` when empty. */
function optional(get: () => string | null, set: (v: string | null) => void) {
  return computed({ get: () => get() ?? '', set: (v: string) => set(v.trim() ? v : null) })
}
const localCwd = optional(
  () => local.value?.cwd ?? null,
  (v) => local.value && (local.value.cwd = v),
)
const wslDistro = optional(
  () => local.value?.wslDistro ?? null,
  (v) => local.value && (local.value.wslDistro = v),
)

async function browseDir(set: (v: string) => void, current: string) {
  const picked = await openFileDialog({
    title: t('stepEditor.pickDir'),
    directory: true,
    multiple: false,
    defaultPath: current || undefined,
  })
  if (typeof picked === 'string') set(picked)
}

// ---- Upload --------------------------------------------------------------------------------

const upload = computed(() =>
  step.value.spec.type === 'upload' ? (step.value.spec as UploadSpec) : null,
)
const sourceKindOptions = computed(() => [
  { value: 'file' as const, label: t('stepEditor.file') },
  { value: 'directory' as const, label: t('stepEditor.directory') },
])
const conflictOptions = computed(() => [
  { value: 'error' as const, label: t('stepEditor.conflictError') },
  { value: 'overwrite' as const, label: t('stepEditor.overwrite') },
])
async function browseSource() {
  if (!upload.value) return
  const directory = upload.value.sourceKind === 'directory'
  const picked = await openFileDialog({
    title: directory ? t('stepEditor.pickUploadDir') : t('stepEditor.pickUploadFile'),
    directory,
    multiple: false,
    defaultPath: upload.value.source || undefined,
  })
  if (typeof picked === 'string' && upload.value) upload.value.source = picked
}

// ---- Remote --------------------------------------------------------------------------------

const remote = computed(() =>
  step.value.spec.type === 'remote' ? (step.value.spec as RemoteSpec) : null,
)
const modeOptions = computed(() => [
  { value: 'script' as const, label: t('stepEditor.script') },
  { value: 'program' as const, label: t('stepEditor.program') },
])

// IXD-10: both forms keep their own draft while the panel is open; only the current one is
// saved. Nothing is converted between them.
const scriptDraft = ref('')
const programDraft = ref<{ executable: string; args: string[] }>({ executable: '', args: [] })
const argsText = ref('')
watch(
  () => step.value.id,
  () => {
    const command = remote.value?.command
    scriptDraft.value = command?.mode === 'script' ? command.script : ''
    programDraft.value =
      command?.mode === 'program'
        ? { executable: command.executable, args: [...command.args] }
        : { executable: '', args: [] }
    argsText.value = programDraft.value.args.join('\n')
  },
  { immediate: true },
)

const mode = computed({
  get: () => remote.value?.command.mode ?? 'script',
  set(next: RemoteCommand['mode']) {
    const r = remote.value
    if (!r || r.command.mode === next) return
    if (r.command.mode === 'script') scriptDraft.value = r.command.script
    else programDraft.value = { executable: r.command.executable, args: [...r.command.args] }
    r.command =
      next === 'script'
        ? { mode: 'script', script: scriptDraft.value }
        : { mode: 'program', ...programDraft.value }
    argsText.value = next === 'program' ? programDraft.value.args.join('\n') : argsText.value
  },
})
const switchedMode = ref(false)
watch(mode, (_, before) => {
  if (before) switchedMode.value = true
})
watch(
  () => step.value.id,
  () => (switchedMode.value = false),
)

const script = computed({
  get: () => (remote.value?.command.mode === 'script' ? remote.value.command.script : ''),
  set(v: string) {
    if (remote.value?.command.mode === 'script') remote.value.command.script = v
  },
})
const executable = computed({
  get: () => (remote.value?.command.mode === 'program' ? remote.value.command.executable : ''),
  set(v: string) {
    if (remote.value?.command.mode === 'program') remote.value.command.executable = v
  },
})
watch(argsText, (text) => {
  if (remote.value?.command.mode === 'program') {
    remote.value.command.args = text.split(/\r?\n/).filter((line) => line !== '')
  }
})
const remoteCwd = optional(
  () => remote.value?.cwd ?? null,
  (v) => remote.value && (remote.value.cwd = v),
)

type PrivilegeMode = StepPrivilege['mode']
const privilegeOptions = computed(() => [
  { value: 'inherit' as const, label: t('stepEditor.privInherit') },
  ...(['none', 'sudoNonInteractive', 'sudoPassword'] as const).map((value) => ({
    value,
    label: t(`servers.dialog.privilege.${value}`),
  })),
])
const runAs = ref('root')
watch(
  () => step.value.id,
  () => {
    const p = remote.value?.privilege
    runAs.value = p && 'runAs' in p ? p.runAs : 'root'
  },
  { immediate: true },
)
const privilegeMode = computed<PrivilegeMode>({
  get: () => remote.value?.privilege.mode ?? 'inherit',
  set(m) {
    if (!remote.value) return
    remote.value.privilege =
      m === 'inherit' || m === 'none' ? { mode: m } : { mode: m, runAs: runAs.value }
  },
})
watch(runAs, (user) => {
  const p = remote.value?.privilege
  if (p && 'runAs' in p) p.runAs = user
})
const usesSudo = computed(() => {
  const r = remote.value
  if (!r) return false
  if (r.privilege.mode === 'inherit') return props.targets.some((s) => s.privilege.mode !== 'none')
  return r.privilege.mode !== 'none'
})
</script>

<template>
  <div class="step-editor">
    <header class="head">
      <span class="kind">{{ kindLabel }}</span>
    </header>

    <div class="grid">
      <UiField
        v-slot="{ id, describedBy, invalid }"
        :label="t('common.name')"
        :error="errorAt('name')"
        required
      >
        <UiTextInput :id="id" v-model="step.name" :invalid="invalid" :described-by="describedBy" />
      </UiField>
      <UiField
        v-if="step.spec.type !== 'confirm'"
        v-slot="{ id, describedBy, invalid }"
        :label="t('stepEditor.timeout')"
        :error="errorAt('timeoutSec')"
      >
        <UiNumberInput
          :id="id"
          v-model="step.timeoutSec"
          :min="1"
          :invalid="invalid"
          :described-by="describedBy"
        />
      </UiField>
    </div>
    <UiField
      v-if="step.spec.type !== 'confirm'"
      v-slot="{ id, describedBy, invalid }"
      :label="t('pipelines.settings.policy')"
      :hint="failureHint"
      :error="errorAt('failurePolicy')"
    >
      <UiSelect
        :id="id"
        v-model="step.failurePolicy"
        class="policy"
        :options="failureOptions"
        :invalid="invalid"
        :described-by="describedBy"
      />
    </UiField>

    <!-- Local command (STEP-02) -->
    <template v-if="local">
      <UiField
        v-slot="{ describedBy, invalid }"
        :label="t('stepEditor.command')"
        :error="errorAt('spec.command')"
        required
      >
        <div class="with-menu">
          <UiTextarea
            :id="fid('command')"
            v-model="local.command"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
          <VariableMenu
            v-model="local.command"
            :field-id="fid('command')"
            :field-label="t('stepEditor.command')"
            local
          />
        </div>
      </UiField>
      <div class="grid">
        <UiField v-slot="{ id }" label="Shell" :hint="programNote(local.shell)">
          <UiSelect :id="id" v-model="local.shell" :options="shellOptions" />
        </UiField>
        <UiField
          v-slot="{ id }"
          :label="t('stepEditor.encoding')"
          :hint="t('stepEditor.encodingHint')"
        >
          <UiSelect :id="id" v-model="local.outputEncoding" :options="encodingOptions" />
        </UiField>
      </div>
      <UiField
        v-if="local.shell === 'wsl'"
        v-slot="{ id, describedBy, invalid }"
        :label="t('stepEditor.wslDistro')"
        :hint="distros && !distros.length ? t('stepEditor.noDistros') : undefined"
        :error="errorAt('spec.wslDistro')"
        required
      >
        <UiSelect
          v-if="distroOptions.length"
          :id="id"
          v-model="wslDistro"
          :options="[{ value: '', label: t('stepEditor.choose') }, ...distroOptions]"
          :invalid="invalid"
          :described-by="describedBy"
        />
        <UiTextInput
          v-else
          :id="id"
          v-model="wslDistro"
          mono
          placeholder="Ubuntu-22.04"
          :invalid="invalid"
          :described-by="describedBy"
        />
      </UiField>
      <UiField
        v-slot="{ describedBy, invalid }"
        :label="t('stepEditor.cwd')"
        :hint="local.shell === 'wsl' ? t('stepEditor.wslCwdHint') : t('stepEditor.cwdHint')"
        :error="errorAt('spec.cwd')"
      >
        <div class="with-menu">
          <UiTextInput
            :id="fid('cwd')"
            v-model="localCwd"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
          <UiIconButton
            v-if="local.shell !== 'wsl'"
            tonal
            :icon="FolderOpen"
            :label="t('stepEditor.browseCwd')"
            @click="browseDir((v) => (localCwd = v), localCwd)"
          />
          <VariableMenu
            v-model="localCwd"
            :field-id="fid('cwd')"
            :field-label="t('stepEditor.cwd')"
            local
          />
        </div>
      </UiField>
    </template>

    <!-- Upload (STEP-03) -->
    <template v-if="upload">
      <UiField :label="t('stepEditor.sourceKind')" group>
        <UiSegmented
          v-model="upload.sourceKind"
          :options="sourceKindOptions"
          :label="t('stepEditor.sourceKind')"
        />
      </UiField>
      <UiField
        v-slot="{ describedBy, invalid }"
        :label="
          upload.sourceKind === 'directory' ? t('stepEditor.localDir') : t('stepEditor.localFile')
        "
        :hint="t('stepEditor.sourceHint')"
        :error="errorAt('spec.source')"
        required
      >
        <div class="with-menu">
          <UiTextInput
            :id="fid('source')"
            v-model="upload.source"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
          <UiIconButton
            tonal
            :icon="FolderOpen"
            :label="t('stepEditor.browseSource')"
            @click="browseSource"
          />
          <VariableMenu
            v-model="upload.source"
            :field-id="fid('source')"
            :field-label="t('stepEditor.localPath')"
            local
          />
        </div>
      </UiField>
      <UiField
        v-slot="{ describedBy, invalid }"
        :label="t('stepEditor.target')"
        :hint="
          upload.sourceKind === 'directory'
            ? t('stepEditor.targetDirHint')
            : t('stepEditor.targetFileHint')
        "
        :error="errorAt('spec.target')"
        required
      >
        <div class="with-menu">
          <UiTextInput
            :id="fid('target')"
            v-model="upload.target"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
          <VariableMenu
            v-model="upload.target"
            :field-id="fid('target')"
            :field-label="t('stepEditor.target')"
          />
        </div>
      </UiField>
      <UiField v-slot="{ id }" :label="t('stepEditor.conflict')">
        <UiSelect :id="id" v-model="upload.conflict" class="policy" :options="conflictOptions" />
      </UiField>
    </template>

    <!-- Remote command (STEP-05) -->
    <template v-if="remote">
      <UiField :label="t('stepEditor.mode')" group>
        <UiSegmented v-model="mode" :options="modeOptions" :label="t('stepEditor.mode')" />
      </UiField>
      <UiNotice v-if="switchedMode" tone="warning">
        {{ t('stepEditor.modeSwitched') }}
      </UiNotice>
      <UiField
        v-if="mode === 'script'"
        v-slot="{ describedBy, invalid }"
        :label="t('stepEditor.script')"
        :hint="t('stepEditor.scriptHint')"
        :error="errorAt('spec.command.script')"
        required
      >
        <div class="with-menu">
          <UiTextarea
            :id="fid('script')"
            v-model="script"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
          <VariableMenu
            v-model="script"
            :field-id="fid('script')"
            :field-label="t('stepEditor.script')"
          />
        </div>
      </UiField>
      <template v-else>
        <UiField
          v-slot="{ describedBy, invalid }"
          :label="t('stepEditor.executable')"
          :hint="t('stepEditor.executableHint')"
          :error="errorAt('spec.command.executable')"
          required
        >
          <div class="with-menu">
            <UiTextInput
              :id="fid('exe')"
              v-model="executable"
              mono
              :invalid="invalid"
              :described-by="describedBy"
            />
            <VariableMenu
              v-model="executable"
              :field-id="fid('exe')"
              :field-label="t('stepEditor.executable')"
            />
          </div>
        </UiField>
        <UiField
          v-slot="{ describedBy, invalid }"
          :label="t('stepEditor.args')"
          :hint="t('stepEditor.argsHint')"
          :error="errorAt('spec.command.args')"
        >
          <div class="with-menu">
            <UiTextarea
              :id="fid('args')"
              v-model="argsText"
              mono
              :rows="4"
              :invalid="invalid"
              :described-by="describedBy"
            />
            <VariableMenu
              v-model="argsText"
              :field-id="fid('args')"
              :field-label="t('stepEditor.args')"
            />
          </div>
        </UiField>
      </template>
      <UiField
        v-slot="{ describedBy, invalid }"
        :label="t('stepEditor.remoteCwd')"
        :hint="t('stepEditor.remoteCwdHint')"
        :error="errorAt('spec.cwd')"
      >
        <div class="with-menu">
          <UiTextInput
            :id="fid('rcwd')"
            v-model="remoteCwd"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
          <VariableMenu
            v-model="remoteCwd"
            :field-id="fid('rcwd')"
            :field-label="t('stepEditor.remoteCwd')"
          />
        </div>
      </UiField>
      <div class="grid">
        <UiField v-slot="{ id }" :label="t('stepEditor.privilege')">
          <UiSelect :id="id" v-model="privilegeMode" :options="privilegeOptions" />
        </UiField>
        <UiField
          v-if="privilegeMode === 'sudoNonInteractive' || privilegeMode === 'sudoPassword'"
          v-slot="{ id, describedBy, invalid }"
          :label="t('servers.dialog.runAs')"
          :error="errorAt('spec.privilege.runAs')"
          required
        >
          <UiTextInput
            :id="id"
            v-model="runAs"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
        </UiField>
      </div>
      <p v-if="mode === 'script' && usesSudo" class="note">
        {{ t('stepEditor.sudoScriptNote') }}
      </p>
      <div v-if="targets.length" class="per-target">
        <p class="per-target-title">{{ t('stepEditor.perTarget') }}</p>
        <ul>
          <li v-for="server in targets" :key="server.id">
            <span class="server">{{ server.name }}</span>
            {{ effectivePrivilege(remote.privilege, server) }}
          </li>
        </ul>
      </div>
    </template>

    <!-- Manual confirmation (STEP-06) -->
    <template v-if="step.spec.type === 'confirm'">
      <UiField
        v-slot="{ id, describedBy, invalid }"
        :label="t('stepEditor.prompt')"
        :hint="t('stepEditor.promptHint')"
        :error="errorAt('spec.prompt')"
        required
      >
        <UiTextarea
          :id="id"
          v-model="step.spec.prompt"
          :rows="3"
          :invalid="invalid"
          :described-by="describedBy"
        />
      </UiField>
    </template>
  </div>
</template>

<style scoped>
.step-editor {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.kind {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-4);
}

.policy {
  max-width: 320px;
}

.with-menu {
  display: flex;
  align-items: flex-start;
  gap: var(--space-1);
}

.with-menu > :first-child {
  flex: 1;
  min-width: 0;
}

.note {
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.per-target {
  padding: var(--space-3);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-surface);
}

.per-target-title {
  margin: 0 0 var(--space-1);
  font: var(--font-label);
}

.per-target ul {
  margin: 0;
  padding-left: var(--space-5);
}

.server {
  margin-right: var(--space-2);
  font-weight: 600;
}
</style>
