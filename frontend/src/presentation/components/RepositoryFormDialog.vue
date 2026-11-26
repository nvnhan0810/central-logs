<script setup lang="ts">
import { ref, watch } from 'vue';

import type { NewLogRepository } from '@/application/logs/log-gateway';
import { directoriesError, nameError, parseDirectories } from '@/presentation/projects/form-rules';

import FormActions from './FormActions.vue';
import ModalDialog from './ModalDialog.vue';

const props = defineProps<{
  open: boolean;
  /** Name of the project the sub-project is added to. */
  projectName: string;
  busy: boolean;
  error: string | null;
}>();
const emit = defineEmits<{ close: []; submit: [input: NewLogRepository] }>();

const name = ref('');
const directories = ref('');
const problem = ref<string | null>(null);

watch(
  () => props.open,
  (open) => {
    if (open) {
      name.value = '';
      directories.value = '';
      problem.value = null;
    }
  },
);

function onSubmit(): void {
  const paths = parseDirectories(directories.value);
  problem.value = nameError(name.value) ?? directoriesError(paths);
  if (problem.value !== null) {
    return;
  }
  emit('submit', { name: name.value.trim(), directories: paths });
}
</script>

<template>
  <ModalDialog :open="open" :title="`Thêm sub-project vào ${projectName}`" @close="emit('close')">
    <form class="form" @submit.prevent="onSubmit">
      <label>
        Tên sub-project
        <input v-model="name" type="text" autocomplete="off" autofocus />
      </label>
      <label>
        Thư mục chứa log (mỗi dòng một đường dẫn tuyệt đối)
        <textarea v-model="directories" rows="4" spellcheck="false" placeholder="/var/log/api" />
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
