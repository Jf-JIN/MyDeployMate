<script setup lang="ts">
import { FolderOpen } from '@lucide/vue'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { computed, ref, shallowRef, toRaw, useId, watch } from 'vue'

import { describeReference } from '@/components/references'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiNumberInput from '@/components/ui/UiNumberInput.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { cloneWorkspace, focusFirstInvalid, useIssues } from '@/composables/workspaceDraft'
import {
  call,
  toAppError,
  type AgentKind,
  type AuthMethod,
  type KeyFormat,
  type PrivilegePolicy,
  type Server,
  type ServerEdit,
} from '@/ipc'
import { useWorkspaceStore } from '@/stores/workspace'

import { describeKeyFormat, parseTags } from './serverForm'

// D01 server form (IXD-16, IXD-17, IXD-20). Edits a copy; cancel leaves the saved server as it
// was. Save runs the Rust edit (bindings an environment change removes) and the normal
// workspace save, so every rule is checked in one place.
const props = defineProps<{ server: Server | null; isNew: boolean }>()
const emit = defineEmits<{ cancel: []; saved: [server: Server] }>()

const store = useWorkspaceStore()
const form = ref<Server | null>(null)
const tagsText = ref('')
const body = ref<HTMLElement | null>(null)
const { issues, at, under } = useIssues()
const error = ref<string | null>(null)
const saving = ref(false)
const attempted = ref(false)
const pending = shallowRef<ServerEdit | null>(null)
const legacyId = useId()

// Non-sensitive values of the methods not currently selected, so switching back restores them
// (IXD-17). Nothing secret is ever typed into this form.
const keyPath = ref('')
const agent = ref<AgentKind>('openssh')
const passwordEnv = ref('')
const runAs = ref('root')

watch(
  () => props.server,
  (server) => {
    form.value = server ? structuredClone(toRaw(server)) : null
    issues.value = []
    error.value = null
    attempted.value = false
    pending.value = null
    if (!server) return
    tagsText.value = server.tags.join(' ')
    const auth = server.auth
    keyPath.value = auth.method === 'keyFile' ? auth.path : ''
    agent.value = auth.method === 'agent' ? auth.agent : 'openssh'
    passwordEnv.value =
      auth.method === 'password' && auth.source.kind === 'envVar' ? auth.source.name : ''
    runAs.value = server.privilege.mode === 'none' ? 'root' : server.privilege.runAs
  },
  { immediate: true },
)

const title = computed(() =>
  props.isNew ? '新增服务器' : `编辑服务器 ${props.server?.name ?? ''}`,
)

const environmentOptions = computed(() =>
  store.environments.map((e) => ({
    value: e.id,
    label: e.protected ? `${e.name}（保护环境）` : e.name,
  })),
)

// ---- Authentication ------------------------------------------------------------------------

type AuthChoice = 'keyFile' | 'agent' | 'passwordPrompt' | 'passwordEnv'
const authOptions = [
  { value: 'keyFile' as const, label: '私钥文件' },
  { value: 'agent' as const, label: 'SSH Agent' },
  { value: 'passwordPrompt' as const, label: '运行时输入密码' },
  { value: 'passwordEnv' as const, label: '密码环境变量' },
]
const agentOptions = [
  { value: 'openssh' as const, label: 'Windows OpenSSH Agent' },
  { value: 'pageant' as const, label: 'Pageant（PuTTY）' },
]

const authChoice = computed<AuthChoice>({
  get() {
    const auth = form.value?.auth
    if (!auth || auth.method === 'keyFile') return 'keyFile'
    if (auth.method === 'agent') return 'agent'
    return auth.source.kind === 'prompt' ? 'passwordPrompt' : 'passwordEnv'
  },
  set(choice) {
    if (!form.value) return
    const auth: AuthMethod =
      choice === 'keyFile'
        ? { method: 'keyFile', path: keyPath.value }
        : choice === 'agent'
          ? { method: 'agent', agent: agent.value }
          : choice === 'passwordPrompt'
            ? { method: 'password', source: { kind: 'prompt' } }
            : { method: 'password', source: { kind: 'envVar', name: passwordEnv.value } }
    form.value.auth = auth
  },
})

watch(keyPath, (path) => {
  if (form.value?.auth.method === 'keyFile') form.value.auth.path = path
})
watch(agent, (kind) => {
  if (form.value?.auth.method === 'agent') form.value.auth.agent = kind
})
watch(passwordEnv, (name) => {
  const auth = form.value?.auth
  if (auth?.method === 'password' && auth.source.kind === 'envVar') auth.source.name = name
})

async function browseKey() {
  const picked = await openFileDialog({
    title: '选择私钥文件',
    multiple: false,
    directory: false,
    defaultPath: keyPath.value || undefined,
    filters: [
      { name: '私钥文件', extensions: ['*'] },
      { name: 'PuTTY 私钥', extensions: ['ppk'] },
    ],
  })
  if (typeof picked === 'string') keyPath.value = picked
}

// IXD-17: say what the chosen file is, or why it cannot be read.
const keyFormat = ref<KeyFormat | null>(null)
const keyError = ref<string | null>(null)
let keyTicket = 0
async function inspectKey() {
  const path = keyPath.value.trim()
  const ticket = ++keyTicket
  keyFormat.value = null
  keyError.value = null
  if (!path || authChoice.value !== 'keyFile') return
  try {
    const format = await call('inspect_key_file', { path })
    if (ticket === keyTicket) keyFormat.value = format
  } catch (e) {
    if (ticket === keyTicket) keyError.value = toAppError(e).message
  }
}
watch([keyPath, authChoice], inspectKey, { immediate: true })

const keyHint = computed(() => {
  if (keyError.value) return keyError.value
  if (keyFormat.value) return describeKeyFormat(keyFormat.value)
  return '支持 OpenSSH 私钥和 PuTTY .ppk；只保存路径，不保存私钥内容。'
})

// IXD-17: whether the variable exists, never its value.
const envPresent = ref<boolean | null>(null)
let envTicket = 0
async function checkEnv() {
  const name = passwordEnv.value.trim()
  const ticket = ++envTicket
  envPresent.value = null
  if (!name || authChoice.value !== 'passwordEnv') return
  const present = await call('env_var_present', { name })
  if (ticket === envTicket) envPresent.value = present
}
watch([passwordEnv, authChoice], checkEnv, { immediate: true })

// ---- Privilege -----------------------------------------------------------------------------

type PrivilegeMode = PrivilegePolicy['mode']
const privilegeOptions = [
  { value: 'none' as const, label: '不提权' },
  { value: 'sudoNonInteractive' as const, label: 'sudo 免交互（sudo -n）' },
  { value: 'sudoPassword' as const, label: 'sudo 运行时输入密码' },
]

const privilegeMode = computed<PrivilegeMode>({
  get: () => form.value?.privilege.mode ?? 'none',
  set(mode) {
    if (!form.value) return
    form.value.privilege = mode === 'none' ? { mode } : { mode, runAs: runAs.value }
  },
})

watch(runAs, (user) => {
  const privilege = form.value?.privilege
  if (privilege && privilege.mode !== 'none') privilege.runAs = user
})

// ---- Tags ----------------------------------------------------------------------------------

watch(tagsText, (text) => {
  if (form.value) form.value.tags = parseTags(text)
})

// ---- Save ----------------------------------------------------------------------------------

const index = ref(-1)
const prefix = computed(() => `servers[${index.value}]`)
function fieldError(field: string): string | null {
  return index.value < 0 ? null : at(`${prefix.value}.${field}`)
}
const tagsError = computed(() =>
  index.value < 0 ? null : (under(`${prefix.value}.tags`)[0]?.message ?? null),
)
const elsewhere = computed(() =>
  issues.value.filter((i) => i.path !== prefix.value && !i.path.startsWith(`${prefix.value}.`)),
)

async function buildEdit(): Promise<ServerEdit | null> {
  if (!form.value || !store.workspace) return null
  const edit = await call('put_server', {
    draft: cloneWorkspace(store.workspace),
    server: structuredClone(toRaw(form.value)),
  })
  index.value = edit.workspace.servers.findIndex((s) => s.id === form.value?.id)
  return edit
}

// After a failed save, re-check as the user types so fixed fields clear (UI-19).
let validating = 0
watch(
  form,
  async () => {
    if (!attempted.value) return
    const ticket = ++validating
    let report
    try {
      const edit = await buildEdit()
      if (!edit) return
      report = await call('validate_workspace_draft', { draft: edit.workspace })
    } catch (e) {
      report = toAppError(e).validation
      if (!report) return
    }
    if (ticket === validating) issues.value = report.issues
  },
  { deep: true },
)

async function onSave() {
  error.value = null
  saving.value = true
  try {
    const edit = await buildEdit().catch(async (e) => {
      await fail(e)
      return null
    })
    if (!edit) return
    // IXD-20: an environment change shows the bindings it removes before anything is saved.
    if (edit.removedBindings.length && !props.isNew) {
      pending.value = edit
      return
    }
    await commit(edit)
  } finally {
    saving.value = false
  }
}

async function commit(edit: ServerEdit) {
  saving.value = true
  try {
    const saved = await store.save(edit.workspace)
    const server = saved.servers.find((s) => s.id === form.value?.id)
    pending.value = null
    if (server) emit('saved', server)
  } catch (e) {
    pending.value = null
    await fail(e)
  } finally {
    saving.value = false
  }
}

// A mistyped field fails already in `put_server`, before anything is saved.
async function fail(e: unknown) {
  const err = toAppError(e)
  attempted.value = true
  if (err.kind === 'validation' && err.validation) {
    // The path of a new server is only known once Rust placed it.
    const match = /^servers\[(\d+)\]/.exec(err.validation.issues[0]?.path ?? '')
    if (index.value < 0 && match) index.value = Number(match[1])
    issues.value = err.validation.issues
    await focusFirstInvalid(body.value)
  } else {
    error.value = err.message
  }
}

const pendingEnvironment = computed(() =>
  form.value ? store.environmentName(form.value.environmentId) : '',
)
</script>

<template>
  <UiDialog :open="!!form" :title="title" width="server" @cancel="emit('cancel')">
    <div v-if="form" ref="body" class="server-form">
      <UiNotice v-if="error" tone="failure">{{ error }}</UiNotice>
      <UiNotice v-if="elsewhere.length" tone="warning">
        配置的其他位置还有问题，需要先修正才能保存：
        <ul class="issues">
          <li v-for="issue in elsewhere" :key="issue.path">{{ issue.message }}</li>
        </ul>
      </UiNotice>

      <section class="group" aria-labelledby="srv-basic">
        <h3 id="srv-basic">基本连接</h3>
        <div class="grid">
          <UiField
            v-slot="{ id, describedBy, invalid }"
            label="名称"
            :error="fieldError('name')"
            required
          >
            <UiTextInput
              :id="id"
              v-model="form.name"
              :invalid="invalid"
              :described-by="describedBy"
              autofocus
            />
          </UiField>
          <UiField
            v-slot="{ id, describedBy, invalid }"
            label="环境"
            :error="fieldError('environmentId')"
            required
          >
            <UiSelect
              :id="id"
              v-model="form.environmentId"
              :options="environmentOptions"
              :invalid="invalid"
              :described-by="describedBy"
            />
          </UiField>
          <UiField
            v-slot="{ id, describedBy, invalid }"
            label="地址"
            hint="域名、IPv4 或 IPv6，不含端口"
            :error="fieldError('host')"
            required
          >
            <UiTextInput
              :id="id"
              v-model="form.host"
              mono
              :invalid="invalid"
              :described-by="describedBy"
            />
          </UiField>
          <div class="grid grid--pair">
            <UiField
              v-slot="{ id, describedBy, invalid }"
              label="端口"
              :error="fieldError('port')"
              required
            >
              <UiNumberInput
                :id="id"
                v-model="form.port"
                :min="1"
                :max="65535"
                :invalid="invalid"
                :described-by="describedBy"
              />
            </UiField>
            <UiField
              v-slot="{ id, describedBy, invalid }"
              label="登录用户"
              :error="fieldError('user')"
              required
            >
              <UiTextInput
                :id="id"
                v-model="form.user"
                mono
                :invalid="invalid"
                :described-by="describedBy"
              />
            </UiField>
          </div>
        </div>
      </section>

      <section class="group" aria-labelledby="srv-auth">
        <h3 id="srv-auth">认证</h3>
        <div class="grid">
          <UiField v-slot="{ id }" label="认证方式">
            <UiSelect :id="id" v-model="authChoice" :options="authOptions" />
          </UiField>

          <UiField
            v-if="authChoice === 'keyFile'"
            v-slot="{ id, describedBy, invalid }"
            label="私钥文件"
            :hint="keyHint"
            :error="fieldError('auth.path')"
            required
          >
            <div class="with-button">
              <UiTextInput
                :id="id"
                v-model="keyPath"
                mono
                placeholder="C:\Users\…\.ssh\id_ed25519"
                :invalid="invalid"
                :described-by="describedBy"
              />
              <UiIconButton :icon="FolderOpen" label="浏览私钥文件" @click="browseKey" />
            </div>
          </UiField>

          <UiField
            v-else-if="authChoice === 'agent'"
            v-slot="{ id }"
            label="Agent"
            :hint="
              agent === 'openssh'
                ? '使用 Windows 的 OpenSSH Authentication Agent 服务中的密钥。'
                : '使用 PuTTY Pageant 中已加载的密钥。'
            "
          >
            <UiSelect :id="id" v-model="agent" :options="agentOptions" />
          </UiField>

          <p v-else-if="authChoice === 'passwordPrompt'" class="note">
            每次连接时询问登录密码，不写入配置。
          </p>

          <UiField
            v-else
            v-slot="{ id, describedBy, invalid }"
            label="环境变量名"
            hint="从本机进程环境读取密码；修改环境变量后需重启应用。"
            :error="fieldError('auth.source.name')"
            required
          >
            <div class="with-button">
              <UiTextInput
                :id="id"
                v-model="passwordEnv"
                mono
                placeholder="DEPLOY_PASSWORD"
                :invalid="invalid"
                :described-by="describedBy"
              />
              <UiTag v-if="envPresent === true" tone="success">已设置</UiTag>
              <UiTag v-else-if="envPresent === false" tone="warning">当前未设置</UiTag>
            </div>
          </UiField>
        </div>
      </section>

      <section class="group" aria-labelledby="srv-priv">
        <h3 id="srv-priv">默认提权</h3>
        <div class="grid">
          <UiField
            v-slot="{ id }"
            label="提权方式"
            hint="步骤可单独覆盖；运行时密码只在执行时询问。"
          >
            <UiSelect :id="id" v-model="privilegeMode" :options="privilegeOptions" />
          </UiField>
          <UiField
            v-if="privilegeMode !== 'none'"
            v-slot="{ id, describedBy, invalid }"
            label="目标用户"
            :error="fieldError('privilege.runAs')"
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
      </section>

      <section class="group" aria-labelledby="srv-compat">
        <h3 id="srv-compat">兼容性</h3>
        <div class="switch-row">
          <UiSwitch :id="legacyId" v-model="form.allowLegacyRsa" />
          <label :for="legacyId">允许旧算法（ssh-rsa / SHA-1）</label>
        </div>
        <p class="note">仅用于无法升级的旧服务器，安全性较低。</p>
      </section>

      <section class="group" aria-labelledby="srv-other">
        <h3 id="srv-other">其他</h3>
        <div class="grid">
          <UiField
            v-slot="{ id, describedBy, invalid }"
            label="标签"
            hint="用空格或逗号分隔"
            :error="tagsError"
          >
            <UiTextInput
              :id="id"
              v-model="tagsText"
              :invalid="invalid"
              :described-by="describedBy"
            />
          </UiField>
          <UiField
            v-slot="{ id, describedBy, invalid }"
            label="连接超时（秒）"
            :error="fieldError('connectTimeoutSec')"
          >
            <UiNumberInput
              :id="id"
              v-model="form.connectTimeoutSec"
              :min="1"
              :invalid="invalid"
              :described-by="describedBy"
            />
          </UiField>
        </div>
        <UiField v-slot="{ id, describedBy, invalid }" label="备注" :error="fieldError('notes')">
          <UiTextarea
            :id="id"
            v-model="form.notes"
            :rows="3"
            :invalid="invalid"
            :described-by="describedBy"
          />
        </UiField>
      </section>
    </div>

    <template #footer>
      <UiButton :disabled="saving" @click="emit('cancel')">取消</UiButton>
      <UiButton variant="primary" :busy="saving" @click="onSave">保存</UiButton>
    </template>
  </UiDialog>

  <UiConfirm
    :open="!!pending"
    title="更改服务器环境"
    confirm-label="更改环境并解除绑定"
    danger
    :busy="saving"
    @cancel="pending = null"
    @confirm="pending && commit(pending)"
  >
    <p>{{ form?.name }} 将移到 {{ pendingEnvironment }}，以下旧环境中的目标绑定会被移除：</p>
    <ul class="issues">
      <li v-for="(r, i) in pending?.removedBindings ?? []" :key="i">
        {{ describeReference(r) }}
      </li>
    </ul>
    <p class="note">取消则不修改环境和绑定。</p>
  </UiConfirm>
</template>

<style scoped>
.server-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.group {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.group h3 {
  margin: 0;
  font: var(--font-section);
}

.grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-4);
}

.grid--pair {
  grid-template-columns: 120px minmax(0, 1fr);
}

.with-button {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.with-button > :first-child {
  flex: 1;
  min-width: 0;
}

.switch-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.note {
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.issues {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-5);
}
</style>
