import { createApp } from 'vue';

import App from './App.vue';
import { createAppServices } from './app/container';
import { AUTH_SERVICE_KEY } from './presentation/auth/auth-injection';
import { createAppRouter } from './presentation/router';

import './style.css';

const services = createAppServices();

const app = createApp(App);
app.provide(AUTH_SERVICE_KEY, services.auth);
app.use(createAppRouter(services.auth));
app.mount('#app');
