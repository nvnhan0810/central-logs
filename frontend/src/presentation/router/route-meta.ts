import 'vue-router';

declare module 'vue-router' {
  interface RouteMeta {
    /** Only signed-in users may open the route. */
    requiresAuth?: boolean;
    /** Only anonymous users may open the route (e.g. the login page). */
    guestOnly?: boolean;
  }
}
