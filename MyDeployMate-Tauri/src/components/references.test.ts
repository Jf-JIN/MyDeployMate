import { describe, expect, it } from 'vitest'

import { describeReference } from './references'

describe('describeReference', () => {
  it('names the project, environment and server of a binding', () => {
    expect(
      describeReference({
        kind: 'targetBinding',
        projectId: 'p',
        projectName: 'account-api',
        environmentId: 'e',
        environmentName: 'staging',
        serverId: 's',
        serverName: 'staging-api-01',
      }),
    ).toBe('项目 account-api 在 staging 中绑定了 staging-api-01')
  })
})
