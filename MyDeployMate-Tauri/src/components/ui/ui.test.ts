import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import { defineComponent, h, nextTick, ref } from 'vue'

import UiButton from './UiButton.vue'
import UiConfirm from './UiConfirm.vue'
import UiDialog from './UiDialog.vue'
import UiField from './UiField.vue'
import UiSegmented from './UiSegmented.vue'
import UiSelect from './UiSelect.vue'
import UiSwitch from './UiSwitch.vue'

afterEach(() => {
  document.body.innerHTML = ''
})

describe('UiButton', () => {
  it('stays focusable but ignores clicks when it has a disabled reason', async () => {
    const wrapper = mount(UiButton, { props: { disabledReason: '已有运行进行中' } })
    const button = wrapper.get('button')
    expect(button.attributes('disabled')).toBeUndefined()
    expect(button.attributes('aria-disabled')).toBe('true')
    expect(wrapper.text()).toContain('已有运行进行中')
    await button.trigger('click')
    expect(wrapper.emitted('click')).toBeUndefined()
  })

  it('does not emit while busy', async () => {
    const wrapper = mount(UiButton, { props: { busy: true } })
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('click')).toBeUndefined()
    expect(wrapper.get('button').attributes('aria-busy')).toBe('true')
  })
})

describe('UiField', () => {
  it('links the error and hint to the control', () => {
    const wrapper = mount(UiField, {
      props: { label: '名称', error: '名称已存在', hint: '1–64 个字符' },
      slots: {
        default: `<template #default="{ id, describedBy }">
          <input :id="id" :aria-describedby="describedBy" />
        </template>`,
      },
    })
    const input = wrapper.get('input')
    expect(wrapper.get('label').attributes('for')).toBe(input.attributes('id'))
    const ids = input.attributes('aria-describedby')!.split(' ')
    expect(ids).toHaveLength(2)
    expect(wrapper.get(`#${ids[0]}`).text()).toBe('名称已存在')
  })
})

describe('UiSwitch', () => {
  it('toggles and reports its state', async () => {
    const value = ref(false)
    const wrapper = mount(UiSwitch, {
      props: {
        modelValue: value.value,
        label: '启用',
        'onUpdate:modelValue': (v: boolean) => (value.value = v),
      },
    })
    await wrapper.get('button').trigger('click')
    expect(value.value).toBe(true)
    expect(wrapper.get('button').attributes('role')).toBe('switch')
  })
})

describe('UiSegmented', () => {
  const options = [
    { value: 'step', label: '步骤优先' },
    { value: 'server', label: '服务器优先' },
    { value: 'other', label: '其他', disabledReason: '不可用' },
  ]

  it('moves with arrow keys and skips disabled segments', async () => {
    const value = ref('server')
    const wrapper = mount(UiSegmented, {
      props: {
        options,
        label: '执行顺序',
        modelValue: value.value,
        'onUpdate:modelValue': (v: string) => (value.value = v),
      },
    })
    const radios = wrapper.findAll('[role="radio"]')
    expect(radios[1]!.attributes('tabindex')).toBe('0')
    expect(radios[0]!.attributes('tabindex')).toBe('-1')
    await radios[1]!.trigger('keydown', { key: 'ArrowRight' })
    expect(value.value).toBe('step')
    await radios[2]!.trigger('click')
    expect(value.value).toBe('step')
  })
})

function dialogHost(cancellable = true) {
  return defineComponent({
    setup() {
      const open = ref(false)
      const cancels = ref(0)
      return () =>
        h('div', [
          h('button', { id: 'trigger', onClick: () => (open.value = true) }, 'open'),
          h(
            UiDialog,
            {
              open: open.value,
              title: '编辑服务器',
              cancellable,
              onCancel: () => {
                cancels.value++
                open.value = false
              },
            },
            {
              default: () => [h('input', { id: 'first' }), h('input', { id: 'second' })],
              footer: () => h('button', { id: 'save' }, '保存'),
            },
          ),
          h('span', { id: 'cancels' }, String(cancels.value)),
        ])
    },
  })
}

describe('UiDialog', () => {
  it('focuses the first field, traps Tab, and restores focus on Esc', async () => {
    const wrapper = mount(dialogHost(), { attachTo: document.body })
    const trigger = document.getElementById('trigger')!
    trigger.focus()
    trigger.click()
    await nextTick()
    await nextTick()
    expect(document.activeElement?.id).toBe('first')

    const save = document.getElementById('save')!
    save.focus()
    save.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(document.activeElement?.getAttribute('aria-label')).toBe('关闭')

    document.activeElement!.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
    await nextTick()
    expect(wrapper.get('#cancels').text()).toBe('1')
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(trigger)
    wrapper.unmount()
  })

  it('ignores Esc and the backdrop when not cancellable', async () => {
    const wrapper = mount(dialogHost(false), { attachTo: document.body })
    document.getElementById('trigger')!.click()
    await nextTick()
    await nextTick()
    document.activeElement!.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
    document
      .querySelector('.backdrop')!
      .dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
    await nextTick()
    expect(wrapper.get('#cancels').text()).toBe('0')
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    wrapper.unmount()
  })
})

describe('UiConfirm', () => {
  it('requires the typed name before confirming', async () => {
    const wrapper = mount(UiConfirm, {
      props: { open: true, title: '确认执行', confirmLabel: '开始执行', requireText: 'production' },
      attachTo: document.body,
    })
    await nextTick()
    const buttons = () =>
      Array.from(document.querySelectorAll<HTMLButtonElement>('.dialog-footer button'))
    const confirm = () => buttons()[1]!
    confirm().click()
    expect(wrapper.emitted('confirm')).toBeUndefined()
    expect(confirm().getAttribute('aria-disabled')).toBe('true')

    const input = document.querySelector<HTMLInputElement>('.confirm-type input')!
    input.value = 'production'
    input.dispatchEvent(new Event('input'))
    await nextTick()
    confirm().click()
    expect(wrapper.emitted('confirm')).toHaveLength(1)
    wrapper.unmount()
  })
})

describe('UiSelect', () => {
  const options = [
    { value: 'ps', label: 'PowerShell' },
    { value: 'cmd', label: 'cmd', disabledReason: '不可用' },
    { value: 'wsl', label: 'WSL' },
  ]

  function mountSelect() {
    const value = ref('ps')
    const wrapper = mount(UiSelect, {
      attachTo: document.body,
      props: {
        modelValue: value.value,
        options,
        ariaLabel: 'Shell',
        'onUpdate:modelValue': (v: string | number) => (value.value = String(v)),
      },
    })
    return { wrapper, value }
  }

  it('picks with the keyboard and skips disabled options', async () => {
    const { wrapper, value } = mountSelect()
    const trigger = wrapper.get('[role=combobox]')
    await trigger.trigger('keydown', { key: 'ArrowDown' })
    expect(trigger.attributes('aria-expanded')).toBe('true')
    const listbox = document.querySelector('[role=listbox]')!
    expect(listbox.querySelectorAll('[role=option]')).toHaveLength(3)
    await trigger.trigger('keydown', { key: 'ArrowDown' })
    const active = trigger.attributes('aria-activedescendant')!
    expect(document.getElementById(active)!.textContent).toContain('WSL')
    await trigger.trigger('keydown', { key: 'Enter' })
    expect(value.value).toBe('wsl')
    expect(document.querySelector('[role=listbox]')).toBeNull()
    wrapper.unmount()
  })

  it('closes on Escape without changing the value', async () => {
    const { wrapper, value } = mountSelect()
    const trigger = wrapper.get('[role=combobox]')
    await trigger.trigger('click')
    await trigger.trigger('keydown', { key: 'ArrowDown' })
    await trigger.trigger('keydown', { key: 'Escape' })
    expect(trigger.attributes('aria-expanded')).toBe('false')
    expect(value.value).toBe('ps')
    wrapper.unmount()
  })
})
