/**
 * Source language (ARC-38). The structure of this object defines the message keys; `en-US.ts`
 * is type-checked against it. Group keys by page or component and use named placeholders such
 * as `{server}`.
 */
const zhCN = {
  settings: {
    look: {
      title: '外观',
      hint: '立即生效，只保存在本机，不写入配置文件，也不随导出。',
      theme: '主题',
      themeOptions: {
        system: '跟随系统',
        light: '浅色',
        dark: '深色',
      },
      language: '语言',
      languageSystem: '跟随系统',
      englishPreview: '英语译文尚未完成，未翻译的文字仍显示为中文。',
    },
  },
}

export default zhCN

export type MessageSchema = typeof zhCN
