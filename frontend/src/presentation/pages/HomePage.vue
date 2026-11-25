<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';

import { displayName } from '@/domain/auth/user';
import { useAuth } from '@/presentation/auth/use-auth';
import { ROUTE_NAME } from '@/presentation/router/route-names';

const auth = useAuth();
const router = useRouter();

const label = computed(() => {
  const state = auth.state.value;
  return state.status === 'authenticated' ? displayName(state.user) : '';
});

async function onSignOut(): Promise<void> {
  try {
    await auth.signOut();
  } catch (error: unknown) {
    // Local state is already anonymous; the user is sent to the login page regardless.
    console.error('Sign-out request failed', error);
  }
  await router.replace({ name: ROUTE_NAME.login });
}
</script>

<template>
  <main class="home">
    <h1>Central Logs</h1>
    <p>
      Xin chào, <strong>{{ label }}</strong>.
    </p>
    <button type="button" @click="onSignOut">Đăng xuất</button>
  </main>
</template>

<style scoped>
.home {
  display: grid;
  gap: 1rem;
  justify-items: start;
  max-width: 48rem;
  margin: 10vh auto;
}
</style>
