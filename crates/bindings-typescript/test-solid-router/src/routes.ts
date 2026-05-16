import { lazy } from 'solid-js';
import type { RouteDefinition } from '@solidjs/router';

import Home from './pages/home';
import UserPage from './pages/UserPage';

export const routes: RouteDefinition[] = [
  {
    path: '/',
    component: Home,
  },
  {
    path: '/user',
    component: UserPage,
  },
  {
    path: '**',
    component: lazy(() => import('./errors/404')),
  },
];
