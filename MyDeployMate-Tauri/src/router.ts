import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

// First-level navigation (IXD-02). Pages fill in from phase 3.
export const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/projects' },
  { path: '/projects', name: 'projects', component: () => import('./views/ProjectsView.vue') },
  { path: '/servers', name: 'servers', component: () => import('./views/ServersView.vue') },
  { path: '/history', name: 'history', component: () => import('./views/HistoryView.vue') },
  { path: '/templates', name: 'templates', component: () => import('./views/TemplatesView.vue') },
  { path: '/settings', name: 'settings', component: () => import('./views/SettingsView.vue') },
]

export const router = createRouter({ history: createWebHashHistory(), routes })
