<script setup lang="ts">
import {
  CircleCheck,
  CircleHelp,
  CircleX,
  Clock,
  Copy,
  KeyRound,
  LoaderCircle,
  OctagonX,
  PauseCircle,
  Play,
  Save,
  SkipForward,
  Trash2,
} from '@lucide/vue'
import { ref } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiNumberInput from '@/components/ui/UiNumberInput.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useToastStore } from '@/stores/toast'

// Development-only page covering the UI spec §7 state matrix (UI-32). Not part of the product.
const toast = useToastStore()

const name = ref('api-prod-01')
const badName = ref('api prod 01')
const password = ref('secret')
const port = ref(22)
const shell = ref<'powershell' | 'cmd' | 'wsl'>('powershell')
const order = ref<'stepFirst' | 'serverFirst'>('stepFirst')
const command = ref('npm ci\nnpm run build')
const enabled = ref(true)
const selected = ref(true)
const dialogOpen = ref(false)
const confirmOpen = ref(false)

const shells = [
  { value: 'powershell' as const, label: 'PowerShell' },
  { value: 'cmd' as const, label: 'cmd' },
  { value: 'wsl' as const, label: 'WSL', disabledReason: '未检测到 WSL' },
]
const orders = [
  { value: 'stepFirst' as const, label: '步骤优先' },
  { value: 'serverFirst' as const, label: '服务器优先' },
]

const states = [
  { label: '排队', tone: 'neutral', icon: Clock },
  { label: '运行中', tone: 'running', icon: LoaderCircle, spin: true },
  { label: '等待确认', tone: 'warning', icon: PauseCircle },
  { label: '等待凭据', tone: 'warning', icon: KeyRound },
  { label: '成功', tone: 'success', icon: CircleCheck },
  { label: '失败', tone: 'failure', icon: CircleX },
  { label: '已取消', tone: 'neutral', icon: OctagonX },
  { label: '已跳过', tone: 'neutral', icon: SkipForward },
  { label: '结果未知', tone: 'unknown', icon: CircleHelp },
] as const
</script>

<template>
  <div class="page">
    <h1>组件状态</h1>
    <p class="lead">开发模式页面，对照 UI 规范第 7 节检查各组件状态。</p>

    <section>
      <h2>按钮</h2>
      <div class="row">
        <UiButton variant="primary" :icon="Play">执行</UiButton>
        <UiButton :icon="Save">保存</UiButton>
        <UiButton variant="danger">备份并替换配置</UiButton>
        <UiButton variant="ghost">更多</UiButton>
        <UiButton variant="primary" busy>正在保存</UiButton>
        <UiButton variant="primary" disabled-reason="已有运行进行中">执行</UiButton>
        <UiButton compact>紧凑</UiButton>
      </div>
      <div class="row">
        <UiIconButton :icon="Copy" label="复制步骤" />
        <UiIconButton :icon="Trash2" label="删除步骤" />
        <UiIconButton :icon="Copy" label="上移" disabled-reason="已经是第一步" />
        <UiIconButton :icon="Save" label="保存" toolbar />
      </div>
    </section>

    <section>
      <h2>输入</h2>
      <div class="grid">
        <UiField v-slot="{ id, describedBy }" label="名称" hint="1–64 个字符" required>
          <UiTextInput :id="id" v-model="name" :described-by="describedBy" />
        </UiField>
        <UiField
          v-slot="{ id, describedBy, invalid }"
          label="名称（错误）"
          error="名称只能包含字母、数字、- 和 _"
        >
          <UiTextInput :id="id" v-model="badName" :invalid="invalid" :described-by="describedBy" />
        </UiField>
        <UiField v-slot="{ id }" label="只读">
          <UiTextInput :id="id" model-value="SHA256:3f9c…" readonly mono />
        </UiField>
        <UiField v-slot="{ id }" label="禁用" hint="私钥认证时不使用密码">
          <UiTextInput :id="id" model-value="" disabled />
        </UiField>
        <UiField v-slot="{ id }" label="密码">
          <UiTextInput :id="id" v-model="password" type="password" />
        </UiField>
        <UiField v-slot="{ id }" label="端口">
          <UiNumberInput :id="id" v-model="port" :min="1" :max="65535" />
        </UiField>
        <UiField v-slot="{ id }" label="Shell">
          <UiSelect :id="id" v-model="shell" :options="shells" />
        </UiField>
        <UiField label="执行顺序" group>
          <UiSegmented v-model="order" :options="orders" label="执行顺序" />
        </UiField>
      </div>
      <UiField v-slot="{ id }" label="命令">
        <UiTextarea :id="id" v-model="command" mono />
      </UiField>
      <div class="row">
        <UiSwitch v-model="enabled" label="启用步骤" />
        <span>{{ enabled ? '已启用' : '已禁用' }}</span>
        <UiSwitch :model-value="false" label="禁用的开关" disabled />
        <UiCheckbox v-model="selected" label="api-prod-01" />
        <UiCheckbox :model-value="false" label="不可选" disabled />
      </div>
    </section>

    <section>
      <h2>状态标签</h2>
      <div class="row">
        <UiTag v-for="s in states" :key="s.label" :tone="s.tone" :icon="s.icon" :spin="'spin' in s">
          {{ s.label }}
        </UiTag>
        <UiTag tone="warning">失败继续</UiTag>
        <UiTag tone="warning">跳过该服务器</UiTag>
        <UiTag>已禁用</UiTag>
      </div>
    </section>

    <section>
      <h2>提示</h2>
      <UiNotice tone="running">当前有运行进行中，这里的更改将在下次运行生效。</UiNotice>
      <UiNotice tone="warning">production 是保护环境，执行前需要键入环境名。</UiNotice>
      <UiNotice tone="failure">
        主机密钥与已信任的记录不一致，连接已阻断。
        <template #actions><UiButton compact>查看指纹</UiButton></template>
      </UiNotice>
      <UiNotice tone="unknown">远程任务的结果未知，请人工核验。</UiNotice>
      <UiNotice tone="success" dismissible>配置已导入。</UiNotice>
      <div class="row">
        <UiButton @click="toast.show('已保存')">显示 3 秒提示</UiButton>
      </div>
    </section>

    <section>
      <h2>弹窗</h2>
      <div class="row">
        <UiButton @click="dialogOpen = true">打开普通弹窗</UiButton>
        <UiButton variant="danger" @click="confirmOpen = true">打开键入确认</UiButton>
      </div>
    </section>

    <UiDialog
      :open="dialogOpen"
      title="编辑服务器"
      description="Esc 或关闭按钮触发取消"
      width="server"
      @cancel="dialogOpen = false"
    >
      <div class="grid">
        <UiField v-slot="{ id }" label="名称"><UiTextInput :id="id" v-model="name" /></UiField>
        <UiField v-slot="{ id }" label="端口"><UiNumberInput :id="id" v-model="port" /></UiField>
      </div>
      <template #footer>
        <UiButton @click="dialogOpen = false">取消</UiButton>
        <UiButton variant="primary" @click="confirmOpen = true">保存</UiButton>
      </template>
    </UiDialog>

    <UiConfirm
      :open="confirmOpen"
      title="确认执行到 production"
      confirm-label="开始执行"
      require-text="production"
      @cancel="confirmOpen = false"
      @confirm="confirmOpen = false"
    >
      <p>将在 3 台服务器上执行 10 项任务。已完成的操作不会自动撤销。</p>
    </UiConfirm>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  max-width: 1080px;
  padding: var(--space-5);
}

.lead {
  margin: calc(-1 * var(--space-4)) 0 0;
  color: var(--text-secondary);
}

section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding-top: var(--space-4);
  border-top: 1px solid var(--border-subtle);
}

.row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-4);
}
</style>
