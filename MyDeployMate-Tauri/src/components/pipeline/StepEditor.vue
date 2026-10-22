<script setup lang="ts">
import { FolderOpen } from '@lucide/vue'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, ref, useId, watch } from 'vue'

import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiNumberInput from '@/components/ui/UiNumberInput.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
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

const kindLabel = computed(
  () =>
    ({ local: '本地命令', upload: '上传文件', remote: '远程命令', confirm: '人工确认' })[
      step.value.spec.type
    ],
)

// ---- Common ----------------------------------------------------------------------------------

const PIPELINE_POLICY = { stop: '失败停止', continue: '失败继续', skipServer: '跳过该服务器' }
const failureOptions = computed(() => {
  const local = step.value.spec.type === 'local'
  const stepFirst = props.pipeline.executionOrder !== 'serverFirst'
  return [
    {
      value: 'inherit' as StepFailurePolicy,
      label: `继承流程（${PIPELINE_POLICY[props.pipeline.failurePolicy]}）`,
    },
    { value: 'stop' as StepFailurePolicy, label: '失败停止' },
    { value: 'continue' as StepFailurePolicy, label: '失败继续' },
    {
      value: 'skipServer' as StepFailurePolicy,
      label: '跳过该服务器',
      disabledReason: local ? '本地命令没有目标服务器' : stepFirst ? '仅服务器优先可用' : undefined,
    },
  ]
})

const failureHint = computed(() => {
  const policy =
    step.value.failurePolicy === 'inherit' ? props.pipeline.failurePolicy : step.value.failurePolicy
  return {
    stop: '失败后不再开始后续任务。',
    continue: '失败后继续后续任务，整次运行最终记为失败。',
    skipServer: '该服务器的后续任务记为跳过，其他服务器继续，整次运行最终记为失败。',
  }[policy]
})

// ---- Local ---------------------------------------------------------------------------------

const local = computed(() =>
  step.value.spec.type === 'local' ? (step.value.spec as LocalSpec) : null,
)
const shellOptions = [
  { value: 'powershell' as const, label: 'Windows PowerShell' },
  { value: 'pwsh' as const, label: 'PowerShell 7（pwsh）' },
  { value: 'cmd' as const, label: 'cmd' },
  { value: 'wsl' as const, label: 'WSL' },
]
const encodingOptions = [
  { value: 'auto' as const, label: '自动' },
  { value: 'utf8' as const, label: 'UTF-8' },
  { value: 'gbk' as const, label: 'GBK' },
]
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
    title: '选择目录',
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
const sourceKindOptions = [
  { value: 'file' as const, label: '文件' },
  { value: 'directory' as const, label: '目录' },
]
const conflictOptions = [
  { value: 'error' as const, label: '目标已存在时报错' },
  { value: 'overwrite' as const, label: '覆盖' },
]
async function browseSource() {
  if (!upload.value) return
  const directory = upload.value.sourceKind === 'directory'
  const picked = await openFileDialog({
    title: directory ? '选择要上传的目录' : '选择要上传的文件',
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
const modeOptions = [
  { value: 'script' as const, label: '脚本' },
  { value: 'program' as const, label: '程序及参数' },
]

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
const privilegeOptions = [
  { value: 'inherit' as const, label: '继承服务器默认' },
  { value: 'none' as const, label: '不提权' },
  { value: 'sudoNonInteractive' as const, label: 'sudo 免交互（sudo -n）' },
  { value: 'sudoPassword' as const, label: 'sudo 运行时输入密码' },
]
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
      <UiField v-slot="{ id, describedBy, invalid }" label="名称" :error="errorAt('name')" required>
        <UiTextInput :id="id" v-model="step.name" :invalid="invalid" :described-by="describedBy" />
      </UiField>
      <UiField
        v-if="step.spec.type !== 'confirm'"
        v-slot="{ id, describedBy, invalid }"
        label="超时（秒）"
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
      label="失败策略"
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
        label="命令"
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
            field-label="命令"
            local
          />
        </div>
      </UiField>
      <div class="grid">
        <UiField v-slot="{ id }" label="Shell">
          <UiSelect :id="id" v-model="local.shell" :options="shellOptions" />
        </UiField>
        <UiField v-slot="{ id }" label="输出编码" hint="出现乱码时可手动指定">
          <UiSelect :id="id" v-model="local.outputEncoding" :options="encodingOptions" />
        </UiField>
      </div>
      <UiField
        v-if="local.shell === 'wsl'"
        v-slot="{ id, describedBy, invalid }"
        label="WSL 发行版"
        :hint="
          distros && !distros.length ? '未检测到已安装的 WSL 发行版，可手动填写名称。' : undefined
        "
        :error="errorAt('spec.wslDistro')"
        required
      >
        <UiSelect
          v-if="distroOptions.length"
          :id="id"
          v-model="wslDistro"
          :options="[{ value: '', label: '请选择' }, ...distroOptions]"
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
        label="工作目录"
        :hint="
          local.shell === 'wsl'
            ? 'Linux 绝对路径，例如 /home/user/app；不会自动换算 Windows 路径。'
            : '留空使用项目根目录；相对路径以项目根目录为准。'
        "
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
            :icon="FolderOpen"
            label="浏览工作目录"
            @click="browseDir((v) => (localCwd = v), localCwd)"
          />
          <VariableMenu v-model="localCwd" :field-id="fid('cwd')" field-label="工作目录" local />
        </div>
      </UiField>
    </template>

    <!-- Upload (STEP-03) -->
    <template v-if="upload">
      <UiField label="源类型" group>
        <UiSegmented v-model="upload.sourceKind" :options="sourceKindOptions" label="源类型" />
      </UiField>
      <UiField
        v-slot="{ describedBy, invalid }"
        :label="upload.sourceKind === 'directory' ? '本地目录' : '本地文件'"
        hint="相对路径以项目根目录为准；构建产物暂不存在时在执行时检查。"
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
          <UiIconButton :icon="FolderOpen" label="浏览本地路径" @click="browseSource" />
          <VariableMenu
            v-model="upload.source"
            :field-id="fid('source')"
            field-label="本地路径"
            local
          />
        </div>
      </UiField>
      <UiField
        v-slot="{ describedBy, invalid }"
        label="远端目标"
        :hint="
          upload.sourceKind === 'directory'
            ? '远端绝对路径；目录中的内容放到这里。'
            : '远端绝对路径，包含文件名。'
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
          <VariableMenu v-model="upload.target" :field-id="fid('target')" field-label="远端目标" />
        </div>
      </UiField>
      <UiField v-slot="{ id }" label="目标已存在时">
        <UiSelect :id="id" v-model="upload.conflict" class="policy" :options="conflictOptions" />
      </UiField>
    </template>

    <!-- Remote command (STEP-05) -->
    <template v-if="remote">
      <UiField label="写法" group>
        <UiSegmented v-model="mode" :options="modeOptions" label="写法" />
      </UiField>
      <UiNotice v-if="switchedMode" tone="warning">
        切换写法不会转换内容，两种写法的草稿各自保留；保存时只保存当前写法。
      </UiNotice>
      <UiField
        v-if="mode === 'script'"
        v-slot="{ describedBy, invalid }"
        label="脚本"
        hint="由远端 sh -c 执行。"
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
          <VariableMenu v-model="script" :field-id="fid('script')" field-label="脚本" />
        </div>
      </UiField>
      <template v-else>
        <UiField
          v-slot="{ describedBy, invalid }"
          label="可执行文件"
          hint="远端绝对路径，例如 /usr/bin/systemctl"
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
            <VariableMenu v-model="executable" :field-id="fid('exe')" field-label="可执行文件" />
          </div>
        </UiField>
        <UiField
          v-slot="{ describedBy, invalid }"
          label="参数"
          hint="每行一个参数，原样传递，不经过 Shell。"
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
            <VariableMenu v-model="argsText" :field-id="fid('args')" field-label="参数" />
          </div>
        </UiField>
      </template>
      <UiField
        v-slot="{ describedBy, invalid }"
        label="远端目录"
        hint="留空使用登录用户的默认目录；否则填写远端绝对路径。"
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
          <VariableMenu v-model="remoteCwd" :field-id="fid('rcwd')" field-label="远端目录" />
        </div>
      </UiField>
      <div class="grid">
        <UiField v-slot="{ id }" label="提权">
          <UiSelect :id="id" v-model="privilegeMode" :options="privilegeOptions" />
        </UiField>
        <UiField
          v-if="privilegeMode === 'sudoNonInteractive' || privilegeMode === 'sudoPassword'"
          v-slot="{ id, describedBy, invalid }"
          label="目标用户"
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
        脚本写法加 sudo 时以 sudo sh -c 运行，需要服务器授权该用户执行 Shell。
      </p>
      <div v-if="targets.length" class="per-target">
        <p class="per-target-title">当前环境各目标的生效提权</p>
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
        label="确认提示"
        hint="运行到这里时暂停，显示这段提示，等你确认后继续。"
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
