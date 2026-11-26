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

## Logs screen

After sign-in the home page (`/`) is the logs screen (`src/presentation/pages/LogsPage.vue`):

- **Project and sub-project** selectors. A project is `Project`; a sub-project is a backend
  `LogRepository`, which holds one or more absolute directories.
- **Tree** in the sidebar: each registered directory is a root. Directories load their children the first
  time they open. Clicking a file opens it.
- **Viewer**: lines are shown newest first. The search box filters by plain text (case-insensitive) or,
  with **Regex** ticked, by a JavaScript regular expression. At most 1000 matching lines are drawn at once.
- **Live updates**: the open file is polled every 3 seconds. Only bytes past the last offset read are
  requested, and new lines are appended. After an error the next poll starts again from the top of the file.

The backend has no push channel, so this is polling. Opening a file reads it to the end page by page and
keeps every line in memory; very large files are slow to open until the backend offers a tail endpoint.

## Run

Start the backend first (see `../backend/README.md`), then:

```sh
npm install
npm run dev          # http://localhost:5173 (Vite default), calls the backend directly
```

The frontend calls the backend at `http://localhost:8080/api` (set `VITE_API_BASE_URL` to change it).
The backend sends the browser back to `CENTRAL_LOGS_FRONTEND_URL` after login and allows this origin to
call the API (default `http://localhost:3000`), so for this dev server set `CENTRAL_LOGS_FRONTEND_URL=http://localhost:5173`.
For plain-HTTP local development, also set `CENTRAL_LOGS_COOKIE_SECURE=false`.

Use `localhost` for both, not `127.0.0.1` mixed with `localhost`: the session cookie is
`SameSite=Lax`, so the frontend and backend must be same-site, and different hostnames are not.

Environment variables for the frontend:

| Variable            | Default                     | Meaning                                                                |
| ------------------- | --------------------------- | ---------------------------------------------------------------------- |
| `VITE_API_BASE_URL` | `http://localhost:8080/api` | Backend API base URL. Must be same-site with the frontend (see above). |

## Scripts

| Command                | Purpose                                      |
| ---------------------- | -------------------------------------------- |
| `npm run dev`          | Dev server on Vite default port 5173         |
| `npm run build`        | Type check, then production build to `dist/` |
| `npm run typecheck`    | `vue-tsc` over `src` and `tests`             |
| `npm run lint`         | ESLint, zero warnings allowed                |
| `npm run format:check` | Prettier check (`npm run format` to fix)     |
| `npm test`             | Vitest unit tests                            |
