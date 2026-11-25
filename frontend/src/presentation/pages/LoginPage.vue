<script setup lang="ts">
import { computed } from 'vue';
import { useRoute } from 'vue-router';

import { toSafeReturnTo } from '@/domain/auth/return-to';
import { useAuth } from '@/presentation/auth/use-auth';

const route = useRoute();
const auth = useAuth();

const returnTo = computed(() => toSafeReturnTo(route.query.redirect));

function onSignIn(): void {
  auth.signIn(returnTo.value);
}
</script>

<template>
  <main class="login">
    <h1>Central Logs</h1>
    <p>Đăng nhập bằng tài khoản SSO để tiếp tục.</p>
    <button type="button" @click="onSignIn">Đăng nhập với SSO</button>
  </main>
</template>

<style scoped>
.login {
  display: grid;
  gap: 1rem;
  justify-items: start;
  max-width: 24rem;
  margin: 15vh auto;
}
</style>
