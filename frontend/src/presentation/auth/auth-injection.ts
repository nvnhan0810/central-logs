import type { InjectionKey } from 'vue';

import type { AuthService } from '@/application/auth/auth-service';

export const AUTH_SERVICE_KEY: InjectionKey<AuthService> = Symbol('AuthService');
