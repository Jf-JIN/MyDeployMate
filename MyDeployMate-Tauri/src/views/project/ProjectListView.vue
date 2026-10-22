<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import { useWorkspaceStore } from '@/stores/workspace'

// P01 project list. Creating, copying and deleting projects arrive with task 3.4.
const workspace = useWorkspaceStore()
</script>

<template>
  <PageLayout title="项目">
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
          <td>
            <RouterLink :to="{ name: 'project', params: { projectId: project.id } }">
              {{ project.name }}
            </RouterLink>
          </td>
          <td>{{ project.description }}</td>
          <td>{{ workspace.environmentName(project.defaultEnvironmentId) }}</td>
          <td class="num">{{ project.pipelines.length }}</td>
        </tr>
      </tbody>
    </table>
    <EmptyState v-else title="尚无项目" />
  </PageLayout>
</template>
