<script setup lang="ts">
import { ref, watch } from 'vue';

import type { NewProject } from '@/application/logs/log-gateway';
import { nameError } from '@/presentation/projects/form-rules';

import FormActions from './FormActions.vue';
import ModalDialog from './ModalDialog.vue';

const props = defineProps<{
  open: boolean;
  /** A save is in progress. */
  busy: boolean;
  /** Message from the last failed save, if any. */
  error: string | null;
}>();
const emit = defineEmits<{ close: []; submit: [input: NewProject] }>();

const name = ref('');
const description = ref('');
const problem = ref<string | null>(null);

// Each time the popup opens it starts empty.
watch(
  () => props.open,
  (open) => {
    if (open) {
      name.value = '';
      description.value = '';
      problem.value = null;
    }
  },
);

function onSubmit(): void {
  problem.value = nameError(name.value);
  if (problem.value !== null) {
    return;
  }
  emit('submit', { name: name.value.trim(), description: description.value.trim() });
}
</script>

<template>
  <ModalDialog :open="open" title="Thêm dự án" @close="emit('close')">
    <form class="form" @submit.prevent="onSubmit">
      <label>
        Tên dự án
        <input v-model="name" type="text" autocomplete="off" autofocus />
      </label>
      <label>
        Mô tả (không bắt buộc)
        <textarea v-model="description" rows="3" />
      </label>
      <p v-if="problem ?? error" class="error" role="alert">{{ problem ?? error }}</p>
      <FormActions :busy="busy" @cancel="emit('close')" />
    </form>
  </ModalDialog>
</template>

<style scoped>
.form {
  display: grid;
  gap: 0.75rem;
}

.form label {
  display: grid;
  gap: 0.25rem;
  font-size: 0.85rem;
}

.form input,
.form textarea {
  padding: 0.4rem 0.5rem;
  font: inherit;
  border: 1px solid #c3ccd6;
  border-radius: 6px;
  background: #fff;
}

.error {
  margin: 0;
  color: #b42318;
}
</style>
