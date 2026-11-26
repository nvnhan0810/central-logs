<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import type { NewProject, Project } from '@/application/logs/log-gateway';
import { nameError } from '@/presentation/projects/form-rules';

import FormActions from './FormActions.vue';
import ModalDialog from './ModalDialog.vue';

const props = defineProps<{
  open: boolean;
  /** The project being edited, or `null` when a new project is added. */
  initial: Project | null;
  /** A save is in progress. */
  busy: boolean;
  /** Message from the last failed save, if any. */
  error: string | null;
}>();
const emit = defineEmits<{ close: []; submit: [input: NewProject] }>();

const name = ref('');
const description = ref('');
const problem = ref<string | null>(null);
const title = computed(() => (props.initial === null ? 'Thêm dự án' : 'Sửa dự án'));

// Each time the popup opens it starts from the project being edited, or empty when adding.
watch(
  () => props.open,
  (open) => {
    if (open) {
      name.value = props.initial?.name ?? '';
      description.value = props.initial?.description ?? '';
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
  <ModalDialog :open="open" :title="title" @close="emit('close')">
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
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: var(--surface);
}

.error {
  margin: 0;
  color: var(--danger);
}
</style>
