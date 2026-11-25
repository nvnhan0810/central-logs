/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Base URL of the backend API. Defaults to the same-origin `/api` (proxied in dev). */
  readonly VITE_API_BASE_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

// Shim for ESLint's type-aware parser. vue-tsc resolves the real component types.
declare module '*.vue' {
  import type { Component } from 'vue';
  const component: Component;
  export default component;
}
