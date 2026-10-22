import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

// First-level navigation (IXD-02). Pages fill in from phase 3.
export const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/projects' },
  { path: '/projects', name: 'projects', component: () => import('./views/ProjectsView.vue') },
  { path: '/servers', name: 'servers', component: () => import('./views/ServersView.vue') },
  { path: '/history', name: 'history', component: () => import('./views/HistoryView.vue') },
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
