import { createRouter, createWebHistory, type Router } from 'vue-router';

import type { AuthService } from '@/application/auth/auth-service';
import { toSafeReturnTo } from '@/domain/auth/return-to';

import HomePage from '../pages/HomePage.vue';
import LoginPage from '../pages/LoginPage.vue';

import { decideAccess } from './access';
import './route-meta';
import { ROUTE_NAME } from './route-names';

export function createAppRouter(auth: AuthService): Router {
  const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
      {
        path: '/login',
        name: ROUTE_NAME.login,
        component: LoginPage,
        meta: { guestOnly: true, requiresAuth: false },
      },
      {
        path: '/',
        name: ROUTE_NAME.home,
        component: HomePage,
        meta: { guestOnly: false, requiresAuth: true },
      },
      { path: '/:pathMatch(.*)*', redirect: { name: ROUTE_NAME.home } },
    ],
  });

  router.beforeEach(async (to) => {
    if (auth.state.status === 'unknown') {
      await auth.restore();
    }
    const decision = decideAccess(auth.state, {
      requiresAuth: to.meta.requiresAuth === true,
      guestOnly: to.meta.guestOnly === true,
    });
    switch (decision) {
      case 'allow':
        return true;
      case 'to-login':
        return { name: ROUTE_NAME.login, query: { redirect: to.fullPath } };
      case 'to-home':
        // Honour the redirect the user was sent here with, if it is safe.
        return toSafeReturnTo(to.query.redirect);
    }
  });

  return router;
}
