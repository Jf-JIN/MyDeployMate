<script setup lang="ts">
import { Eye, FolderKanban, History, Server } from '@lucide/vue'
import { computed, nextTick, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import { useT } from '@/i18n'
import { useDraftStore } from '@/stores/drafts'
import { useWelcomeStore } from '@/stores/welcome'

const t = useT()

// D06 welcome wizard (IXD-40): four pages that explain the main path, from servers to history.
// It only explains; closing it in any way counts as seen.
const welcome = useWelcomeStore()
const drafts = useDraftStore()
const router = useRouter()

const PAGES = [
  { key: 'servers', icon: Server },
  { key: 'projects', icon: FolderKanban },
  { key: 'preview', icon: Eye },
  { key: 'history', icon: History },
] as const

const index = ref(0)
const primary = ref<InstanceType<typeof UiButton> | null>(null)
const page = computed(() => PAGES[index.value] ?? PAGES[0])
const last = computed(() => index.value === PAGES.length - 1)

watch(
  () => welcome.open,
  (open) => {
    if (open) index.value = 0
  },
)

// A button that disappears with the page (back on the first page) would drop focus to the
// page behind the dialog; move it to the primary button instead.
async function go(step: number) {
  index.value = Math.min(Math.max(index.value + step, 0), PAGES.length - 1)
  await nextTick()
  const el = primary.value?.$el as HTMLElement | undefined
  if (el && !el.closest('.dialog')?.contains(document.activeElement)) el.focus()
}

async function openTemplates() {
  welcome.close()
  if (await drafts.confirmLeave()) void router.push({ name: 'templates' })
}
</script>

<template>
  <UiDialog :open="welcome.open" :title="t('welcome.title')" @cancel="welcome.close()">
    <div class="page" aria-live="polite">
      <span class="icon" aria-hidden="true">
        <component :is="page.icon" :size="28" :stroke-width="1.5" />
      </span>
      <h3 class="page-title">{{ t(`welcome.pages.${page.key}.title`) }}</h3>
      <p class="page-body">{{ t(`welcome.pages.${page.key}.body`) }}</p>
    </div>
    <!-- The footer says the page number in words; the dots only show it. -->
    <ol class="dots" aria-hidden="true">
      <li
        v-for="(p, i) in PAGES"
        :key="p.key"
        class="dot"
        :class="{ 'dot--current': i === index }"
      />
    </ol>

    <template #footer>
      <span class="progress num">
        {{ t('welcome.progress', { n: index + 1, total: PAGES.length }) }}
      </span>
      <UiButton v-if="!last" variant="ghost" @click="welcome.close()">
        {{ t('welcome.skip') }}
      </UiButton>
      <UiButton v-if="index > 0" @click="go(-1)">{{ t('welcome.back') }}</UiButton>
      <UiButton v-if="last" variant="tonal" @click="openTemplates">
        {{ t('welcome.fromTemplate') }}
      </UiButton>
      <!-- One button for "next" and "done", so focus stays on it while paging (IXD-03). -->
      <UiButton ref="primary" variant="primary" autofocus @click="last ? welcome.close() : go(1)">
        {{ last ? t('welcome.done') : t('welcome.next') }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  min-height: 200px;
  padding: var(--space-5) var(--space-4) var(--space-3);
  text-align: center;
}

.icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  margin-bottom: var(--space-2);
  border-radius: 50%;
  background: var(--surface-selected);
  color: var(--action-primary);
}

.page-title {
  margin: 0;
  color: var(--text-primary);
  font: var(--font-section);
}

.page-body {
  max-width: 440px;
  margin: 0;
  color: var(--text-secondary);
}

.dots {
  display: flex;
  justify-content: center;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--border-control);
}

/* The current page is wider as well as coloured, so it does not rely on colour (NFR-02). */
.dot--current {
  width: 20px;
  border-radius: 4px;
  background: var(--action-primary);
}

.progress {
  margin-right: auto;
  color: var(--text-secondary);
}
</style>
