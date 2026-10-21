<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import { useWorkspaceStore } from '@/stores/workspace'

const workspace = useWorkspaceStore()
</script>

<template>
  <section>
    <h1>服务器</h1>
    <table v-if="workspace.servers.length" class="table">
      <thead>
        <tr>
          <th scope="col">名称</th>
          <th scope="col">地址</th>
          <th scope="col" class="num">端口</th>
          <th scope="col">登录用户</th>
          <th scope="col">环境</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="server in workspace.servers" :key="server.id">
          <td>{{ server.name }}</td>
          <td>
            <code>{{ server.host }}</code>
          </td>
          <td class="num">{{ server.port }}</td>
          <td>{{ server.user }}</td>
          <td>{{ workspace.environmentName(server.environmentId) }}</td>
        </tr>
      </tbody>
    </table>
    <EmptyState v-else title="尚无服务器" detail="服务器管理将在后续版本提供。" />
  </section>
</template>
