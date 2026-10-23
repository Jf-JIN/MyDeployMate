import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

import { useDraftStore } from './stores/drafts'

// First-level navigation (IXD-02) and the project's second-level tabs.
export const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/projects' },
  {
    path: '/projects',
    component: () => import('./views/project/ProjectsSection.vue'),
    children: [
      {
        path: '',
        name: 'projects',
        component: () => import('./views/project/ProjectListView.vue'),
      },
      {
        path: ':projectId',
        component: () => import('./views/project/ProjectLayout.vue'),
        children: [
          // Redirects to the last opened pipeline once the workspace is loaded.
          {
            path: '',
            name: 'project',
            component: () => import('./views/project/NoPipelineView.vue'),
          },
          {
            path: 'pipelines',
            name: 'project-pipelines',
            component: () => import('./views/project/NoPipelineView.vue'),
          },
          {
            path: 'pipelines/:pipelineId',
            name: 'pipeline',
            component: () => import('./views/project/PipelineView.vue'),
          },
          {
            path: 'targets',
            name: 'project-targets',
            component: () => import('./views/project/TargetsView.vue'),
          },
          {
            path: 'variables',
            name: 'project-variables',
            component: () => import('./views/project/VariablesView.vue'),
          },
          {
            path: 'settings',
            name: 'project-settings',
            component: () => import('./views/project/ProjectSettingsView.vue'),
          },
        ],
      },
    ],
  },
  { path: '/servers', name: 'servers', component: () => import('./views/ServersView.vue') },
  {
    path: '/history',
    children: [
      { path: '', name: 'history', component: () => import('./views/HistoryView.vue') },
      // P07 run detail (IXD-26); under history so the rail marks it.
      { path: 'runs/:runId', name: 'run', component: () => import('./views/RunView.vue') },
    ],
  },
  { path: '/templates', name: 'templates', component: () => import('./views/TemplatesView.vue') },
  { path: '/settings', name: 'settings', component: () => import('./views/SettingsView.vue') },
  // Component state page for visual checks (UI-32); not shipped in production builds.
  ...(import.meta.env.DEV
    ? [
        {
          path: '/dev/components',
          name: 'dev-components',
          component: () => import('./views/dev/ComponentStatesView.vue'),
        },
      ]
    : []),
]

export const router = createRouter({ history: createWebHashHistory(), routes })

// IXD-03: leaving a page, project, pipeline or tab with unsaved changes asks first.
router.beforeEach(async (to, from) => {
  if (to.fullPath === from.fullPath) return true
  return useDraftStore().confirmLeave()
})
