<script setup lang="ts">
import type { TreeNode, TreeRow } from '@/presentation/logs/tree-model';

defineProps<{
  rows: readonly TreeRow[];
  selectedKey: string | null;
}>();

const emit = defineEmits<{
  open: [node: TreeNode];
}>();
</script>

<template>
  <nav class="tree" aria-label="Cây thư mục log">
    <p v-if="rows.length === 0" class="muted">Chọn sub-project để xem thư mục log.</p>
    <ul v-else class="rows">
      <li v-for="row in rows" :key="row.node.key">
        <button
          type="button"
          class="row"
          :class="{ selected: row.node.key === selectedKey }"
          :style="{ paddingLeft: `${0.5 + row.depth * 1.1}rem` }"
          :title="row.node.directory + (row.node.relativePath ? '/' + row.node.relativePath : '')"
          :aria-expanded="row.node.kind === 'directory' ? row.node.expanded : undefined"
          @click="emit('open', row.node)"
        >
          <span class="icon" aria-hidden="true">
            <template v-if="row.node.kind === 'directory'">{{
              row.node.expanded ? '▾' : '▸'
            }}</template>
            <template v-else>•</template>
          </span>
          <span class="name">{{ row.node.name }}</span>
          <span v-if="row.node.loading" class="muted small">đang tải…</span>
          <span v-else-if="row.node.error !== null" class="error small">{{ row.node.error }}</span>
        </button>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.tree {
  overflow: auto;
  min-height: 0;
}

.rows {
  list-style: none;
  margin: 0;
  padding: 0;
}

.row {
  display: flex;
  align-items: baseline;
  gap: 0.4rem;
  width: 100%;
  padding-top: 0.25rem;
  padding-bottom: 0.25rem;
  border: none;
  border-radius: 0;
  background: transparent;
  color: inherit;
  text-align: left;
}

.row:hover {
  background: #e4e9f0;
}

.row.selected {
  background: #d6e4ff;
}

.icon {
  width: 1em;
  flex: none;
  text-align: center;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.muted {
  color: #6b7785;
}

.small {
  font-size: 0.8rem;
}

.error {
  color: #b42318;
}
</style>
