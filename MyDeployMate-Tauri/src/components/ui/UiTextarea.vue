<script setup lang="ts">
// UI-16: multi-line command input, at least 6 rows / 144 px, resizable; Enter and paste only
// edit text (IXD-11).
const model = defineModel<string>({ required: true })

withDefaults(
  defineProps<{
    id?: string
    rows?: number
    mono?: boolean
    invalid?: boolean
    readonly?: boolean
    disabled?: boolean
    placeholder?: string
    describedBy?: string
  }>(),
  { id: undefined, rows: 6, placeholder: undefined, describedBy: undefined },
)
</script>

<template>
  <textarea
    :id="id"
    v-model="model"
    class="control control--multiline"
    :class="{ 'control--mono': mono }"
    :rows="rows"
    :readonly="readonly"
    :disabled="disabled"
    :placeholder="placeholder"
    :aria-invalid="invalid || undefined"
    :aria-describedby="describedBy"
    spellcheck="false"
    wrap="off"
  />
</template>
