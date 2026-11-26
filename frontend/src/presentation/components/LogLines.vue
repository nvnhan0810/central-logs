<script setup lang="ts">
import { computed, ref } from 'vue';

import type { LogLine } from '@/application/logs/log-gateway';
import { createLineMatcher } from '@/domain/logs/line-search';
import type { FileStatus } from '@/presentation/logs/use-log-file';

/** Matching lines drawn at once. Rendering more would make the page slow; narrow the search instead. */
const MAX_RENDERED = 1000;

const props = defineProps<{
  title: string | null;
  /** Lines in file order (oldest first). They are shown newest first. */
  lines: readonly LogLine[];
  status: FileStatus;
  error: string | null;
}>();

const query = ref('');
const regex = ref(false);

const matcher = computed(() => createLineMatcher(query.value, regex.value ? 'regex' : 'text'));

/** Newest first, filtered by the search. Empty when the pattern is invalid. */
const matching = computed<LogLine[]>(() => {
  const current = matcher.value;
  if (!current.ok) {
    return [];
  }
  const out: LogLine[] = [];
  for (let i = props.lines.length - 1; i >= 0; i--) {
    const line = props.lines[i];
    if (line !== undefined && current.test(line.text)) {
      out.push(line);
    }
  }
  return out;
});

const rendered = computed(() => matching.value.slice(0, MAX_RENDERED));

const statusText = computed(() => {
  switch (props.status) {
    case 'idle':
      return 'Chọn một file log ở thanh bên.';
    case 'loading':
      return 'Đang đọc file…';
    case 'error':
      return 'Lỗi khi cập nhật, sẽ thử lại.';
    case 'live':
      return '';
  }
});
</script>

<template>
  <section class="viewer">
    <header class="bar">
      <strong class="title">{{ title ?? 'Chưa chọn file' }}</strong>
      <span class="counts small muted">
        <template v-if="status === 'live'">
          {{ matching.length }} / {{ lines.length }} dòng
          <span class="live">● đang theo dõi</span>
        </template>
      </span>
    </header>

    <form class="search" role="search" @submit.prevent>
      <input
        v-model="query"
        type="search"
        placeholder="Tìm trong log…"
        aria-label="Tìm kiếm trong log"
      />
      <label class="regex">
        <input v-model="regex" type="checkbox" />
        Regex
      </label>
    </form>
    <p v-if="!matcher.ok" class="error small" role="alert">{{ matcher.error }}</p>

    <p v-if="statusText !== ''" class="muted">{{ statusText }}</p>
    <p v-if="error !== null" class="error small" role="alert">{{ error }}</p>

    <p v-if="matcher.ok && matching.length > MAX_RENDERED" class="muted small">
      Hiển thị {{ MAX_RENDERED }} dòng mới nhất trong {{ matching.length }} dòng khớp. Thu hẹp tìm
      kiếm để xem thêm.
    </p>

    <ol v-if="status === 'live' || lines.length > 0" class="lines">
      <li v-for="(line, index) in rendered" :key="index">
        <code>{{ line.text }}</code>
        <span v-if="line.truncated" class="muted small"> …(cắt bớt)</span>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.viewer {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  gap: 0.5rem;
}

.bar {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 1rem;
  min-width: 0;
}

.title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.search {
  display: flex;
  gap: 0.75rem;
  align-items: center;
}

.search input[type='search'] {
  flex: 1;
  min-width: 0;
  padding: 0.4rem 0.6rem;
  font: inherit;
  border: 1px solid #c3ccd6;
  border-radius: 6px;
}

.regex {
  display: flex;
  gap: 0.3rem;
  align-items: center;
  white-space: nowrap;
}

.lines {
  flex: 1;
  margin: 0;
  padding: 0;
  overflow: auto;
  min-height: 0;
  list-style: none;
  background: #fff;
  border: 1px solid #dfe5ec;
  border-radius: 6px;
}

.lines li {
  padding: 0.15rem 0.6rem;
  border-bottom: 1px solid #f0f3f6;
}

.lines code {
  white-space: pre-wrap;
  word-break: break-all;
  font-size: 0.85rem;
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

.live {
  color: #067647;
}
</style>
