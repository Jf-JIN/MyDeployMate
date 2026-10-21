<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import { useWorkspaceStore } from '@/stores/workspace'

const workspace = useWorkspaceStore()
</script>

<template>
  <section>
    <h1>项目与流程</h1>
    <table v-if="workspace.projects.length" class="table">
      <thead>
        <tr>
          <th scope="col">项目</th>
          <th scope="col">说明</th>
          <th scope="col">默认环境</th>
          <th scope="col" class="num">流程数</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="project in workspace.projects" :key="project.id">
          <td>{{ project.name }}</td>
          <td>{{ project.description }}</td>
          <td>{{ workspace.environmentName(project.defaultEnvironmentId) }}</td>
          <td class="num">{{ project.pipelines.length }}</td>
        </tr>
      </tbody>
    </table>
    <EmptyState v-else title="尚无项目" detail="项目编辑将在后续版本提供。" />
  </section>
</template>
