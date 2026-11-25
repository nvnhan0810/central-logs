# Central Logs — Frontend

Vue 3 + TypeScript (strict) single-page app for the Central Logs backend in `../backend`.
It implements the browser side of SSO login described in `../backend/README.md`.

## Stack

- Vue 3 (`<script setup>`), Vue Router 4, Vite
- TypeScript in strict mode (`noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, …)
- ESLint (flat config, `typescript-eslint` strict type-checked, `eslint-plugin-vue`) and Prettier
- Vitest for unit tests

## Architecture

Dependencies point inward. Domain and application code do not import Vue, the DOM or `fetch`.

| Layer            | Path                                  | Contents                                                                     |
| ---------------- | ------------------------------------- | ---------------------------------------------------------------------------- |
| Domain           | `src/domain`                          | `User`, `AuthState`, return-path rule (`isSafeReturnTo`)                     |
| Application      | `src/application`                     | `AuthGateway` port, `AuthService` use cases (restore, signIn, signOut)       |
| Infrastructure   | `src/infrastructure`                  | `ApiClient` (fetch, cookies, error shape), `HttpAuthGateway`, wire DTO guard |
| Presentation     | `src/presentation`                    | Router and access guard, `useAuth` composable, pages                         |
| Composition root | `src/app/container.ts`, `src/main.ts` | The only place where adapters are wired                                      |

## Login flow

1. A route with `requiresAuth` is opened. On first navigation the app calls `GET /api/auth/me`.
   `401` means anonymous, and the user is sent to `/login?redirect=<path>`.
2. The login button navigates the browser to `/api/auth/login?return_to=<path>`. The backend
   redirects to the identity provider and handles PKCE, the code exchange and the session cookie.
3. The backend redirects back to the frontend at `return_to`. The page reloads, and the guard
   restores the session through `/api/auth/me`.
4. "Đăng xuất" calls `POST /api/auth/logout`, then returns to `/login`.

The frontend never sees the authorization code, PKCE verifier or session id. Its only credential
is the `HttpOnly` cookie `central_logs_session`, which the browser sends automatically.

## Run

Start the backend first (see `../backend/README.md`), then:

```sh
npm install
npm run dev          # http://localhost:3000, /api is proxied to the backend
```

Set `CENTRAL_LOGS_FRONTEND_URL=http://localhost:3000` on the backend. That is the default.
For plain-HTTP local development, also set `CENTRAL_LOGS_COOKIE_SECURE=false`.

Environment variables for the frontend:

| Variable            | Default                 | Meaning                                                           |
| ------------------- | ----------------------- | ----------------------------------------------------------------- |
| `VITE_API_BASE_URL` | `/api`                  | API base URL. Keep it same-origin: the backend has no CORS layer. |
| `VITE_BACKEND_URL`  | `http://127.0.0.1:8080` | Dev-server proxy target (read by `vite.config.ts`)                |

## Scripts

| Command                | Purpose                                      |
| ---------------------- | -------------------------------------------- |
| `npm run dev`          | Dev server with the `/api` proxy             |
| `npm run build`        | Type check, then production build to `dist/` |
| `npm run typecheck`    | `vue-tsc` over `src` and `tests`             |
| `npm run lint`         | ESLint, zero warnings allowed                |
| `npm run format:check` | Prettier check (`npm run format` to fix)     |
| `npm test`             | Vitest unit tests                            |
