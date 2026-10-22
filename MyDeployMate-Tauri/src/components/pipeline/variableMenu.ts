import { inject, provide, type InjectionKey, type Ref } from 'vue'

import type { BuiltinVariable, VariableView } from '@/ipc'

/** What the variable menus of one pipeline editor offer (IXD-11). */
export interface VariableMenuSource {
  variables: Ref<VariableView[]>
  builtins: Ref<BuiltinVariable[]>
  environmentName: Ref<string>
}

const KEY: InjectionKey<VariableMenuSource> = Symbol('variable-menu')

export function provideVariableMenu(source: VariableMenuSource) {
  provide(KEY, source)
}

export function useVariableMenu(): VariableMenuSource | undefined {
  return inject(KEY, undefined)
}
