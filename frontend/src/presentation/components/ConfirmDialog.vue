<script setup lang="ts">
import ModalDialog from './ModalDialog.vue';

defineProps<{
  open: boolean;
  title: string;
  /** What will be deleted and what cannot be undone. */
  message: string;
  busy: boolean;
  /** Message from the last failed delete, if any. */
  error: string | null;
}>();
const emit = defineEmits<{ close: []; confirm: [] }>();
</script>

<template>
  <ModalDialog :open="open" :title="title" @close="emit('close')">
    <div class="confirm">
      <p>{{ message }}</p>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <div class="actions">
        <button type="button" class="secondary" :disabled="busy" @click="emit('close')">Hủy</button>
        <button type="button" class="danger" :disabled="busy" @click="emit('confirm')">
          {{ busy ? 'Đang xóa…' : 'Xóa' }}
        </button>
      </div>
    </div>
  </ModalDialog>
</template>

<style scoped>
.confirm {
  display: grid;
  gap: 0.75rem;
}

.confirm p {
  margin: 0;
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
}

.secondary {
  background: #fff;
  color: #1f2933;
}

.danger {
  background: #b42318;
  border-color: #b42318;
}

.error {
  color: #b42318;
}
</style>
