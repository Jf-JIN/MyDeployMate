import type { MessageSchema } from './zh-CN'

type DeepPartial<T> = { [K in keyof T]?: T[K] extends string ? string : DeepPartial<T[K]> }

/**
 * English (ARC-38). A key that zh-CN.ts does not have fails the typecheck. Until the
 * translation is finished in task 8.4c, keys may be missing and fall back to Chinese; 8.4c
 * changes the type to `MessageSchema` so that a missing key fails too.
 */
const enUS: DeepPartial<MessageSchema> = {
  settings: {
    look: {
      title: 'Appearance',
      hint: 'Applies at once and is kept on this machine only; it is not written to the configuration or exported.',
      theme: 'Theme',
      themeOptions: {
        system: 'Follow system',
        light: 'Light',
        dark: 'Dark',
      },
      language: 'Language',
      languageSystem: 'Follow system',
      englishPreview:
        'The English translation is not finished yet; untranslated text is still shown in Chinese.',
    },
  },
}

export default enUS
