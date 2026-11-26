import { inject, type InjectionKey } from 'vue';

import type { LogGateway } from '@/application/logs/log-gateway';

export const LOG_GATEWAY_KEY: InjectionKey<LogGateway> = Symbol('LogGateway');

/** The logs gateway provided by the composition root. Must run inside `setup()`. */
export function useLogGateway(): LogGateway {
  const gateway = inject(LOG_GATEWAY_KEY);
  if (gateway === undefined) {
    throw new Error('LogGateway is not provided. Call app.provide(LOG_GATEWAY_KEY, ...).');
  }
  return gateway;
}
