<script setup lang="ts">
import { ref, watch } from 'vue';

const props = defineProps<{ open: boolean; title: string }>();
const emit = defineEmits<{ close: [] }>();

const dialog = ref<HTMLDialogElement | null>(null);

watch(
  () => props.open,
  (open) => {
    const el = dialog.value;
    if (el === null) {
      return;
    }
    if (open && !el.open) {
      el.showModal();
    } else if (!open && el.open) {
      el.close();
    }
  },
);

// Escape and the browser's own close both arrive here, so the parent's `open` flag stays in sync.
function onNativeClose(): void {
  emit('close');
}
</script>

<template>
  <dialog ref="dialog" class="modal" :aria-label="title" @close="onNativeClose">
    <div class="modal-body">
      <header class="modal-head">
        <h2>{{ title }}</h2>
        <button type="button" class="ghost" aria-label="Đóng" @click="emit('close')">×</button>
      </header>
      <slot />
    </div>
  </dialog>
</template>

<style scoped>
.modal {
  width: min(32rem, calc(100vw - 2rem));
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  box-shadow: 0 12px 32px var(--shadow);
}

.modal::backdrop {
  background: var(--backdrop);
}

.modal-body {
  padding: 1rem 1.25rem 1.25rem;
}

.modal-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.75rem;
}

.modal-head h2 {
  margin: 0;
  font-size: 1.1rem;
}

.ghost {
  padding: 0.25rem 0.6rem;
  background: transparent;
  color: var(--text);
  border-color: transparent;
  font-size: 1.1rem;
  line-height: 1;
}
</style>
