<script setup lang="ts">
defineProps<{ busy: boolean }>();
const emit = defineEmits<{ cancel: [] }>();
</script>

<template>
  <div class="actions">
    <button type="button" class="secondary" :disabled="busy" @click="emit('cancel')">Hủy</button>
    <button type="submit" :disabled="busy">{{ busy ? 'Đang lưu…' : 'Lưu' }}</button>
  </div>
</template>

<style scoped>
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
}

.secondary {
  background: var(--surface);
  color: var(--text);
}
</style>
