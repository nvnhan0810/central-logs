<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { RouterLink, useRouter } from 'vue-router';

import type { LogRepository, Project } from '@/application/logs/log-gateway';
import { displayName } from '@/domain/auth/user';
import LogLines from '@/presentation/components/LogLines.vue';
import LogTree from '@/presentation/components/LogTree.vue';
import { useAuth } from '@/presentation/auth/use-auth';
import { errorMessage } from '@/presentation/logs/error-message';
import { useLogGateway } from '@/presentation/logs/log-injection';
import { useLogFile } from '@/presentation/logs/use-log-file';
import { useLogTree } from '@/presentation/logs/use-log-tree';
import { nodeKey, type TreeNode } from '@/presentation/logs/tree-model';
import { ROUTE_NAME } from '@/presentation/router/route-names';

/** How often an open file is checked for new content. */
const POLL_INTERVAL_MS = 3000;

const auth = useAuth();
const router = useRouter();
const gateway = useLogGateway();

const tree = useLogTree(gateway);
const file = useLogFile(gateway, POLL_INTERVAL_MS);
const treeRows = tree.rows;
const fileLines = file.lines;
const fileStatus = file.status;
const fileError = file.error;

const projects = ref<Project[]>([]);
const repositories = ref<LogRepository[]>([]);
const projectId = ref<string | null>(null);
const repositoryId = ref<string | null>(null);
const loadError = ref<string | null>(null);

const label = computed(() => {
  const state = auth.state.value;
  return state.status === 'authenticated' ? displayName(state.user) : '';
});

const selectedKey = computed(() => {
  const target = file.target.value;
  return target === null ? null : nodeKey(target.directory, target.relativePath);
});

const fileTitle = computed(() => {
  const target = file.target.value;
  return target === null ? null : `${target.directory}/${target.relativePath}`;
});

async function loadProjects(): Promise<void> {
  try {
    projects.value = [...(await gateway.listProjects())];
    projectId.value = projects.value[0]?.id ?? null;
  } catch (error: unknown) {
    loadError.value = `Không tải được danh sách dự án: ${errorMessage(error)}`;
  }
}

// Changing the project loads its sub-projects, and selects the first one.
watch(projectId, async (id) => {
  repositories.value = [];
  repositoryId.value = null;
  if (id === null) {
    return;
  }
  try {
    const loaded = [...(await gateway.listLogRepositories(id))];
    if (projectId.value !== id) {
      return;
    }
    repositories.value = loaded;
    repositoryId.value = loaded[0]?.id ?? null;
    loadError.value = null;
  } catch (error: unknown) {
    if (projectId.value === id) {
      loadError.value = `Không tải được sub-project: ${errorMessage(error)}`;
    }
  }
});

// Changing the sub-project shows its directories in the tree and closes the open file.
watch(repositoryId, (id) => {
  file.close();
  tree.show(repositories.value.find((repo) => repo.id === id) ?? null);
});

function onOpen(node: TreeNode): void {
  if (node.kind === 'directory') {
    void tree.toggle(node);
    return;
  }
  const repository = repositoryId.value;
  if (repository === null) {
    return;
  }
  void file.open({
    repositoryId: repository,
    directory: node.directory,
    relativePath: node.relativePath,
  });
}

async function onSignOut(): Promise<void> {
  try {
    await auth.signOut();
  } catch (error: unknown) {
    // Local state is already anonymous; the user is sent to the login page regardless.
    console.error('Sign-out request failed', error);
  }
  await router.replace({ name: ROUTE_NAME.login });
}

onMounted(() => {
  void loadProjects();
});
</script>

<template>
  <main class="logs">
    <header class="topbar">
      <h1>Central Logs</h1>
      <div class="who">
        <RouterLink :to="{ name: ROUTE_NAME.projects }">Dự án và sub-project</RouterLink>
        <span>{{ label }}</span>
        <button type="button" @click="onSignOut">Đăng xuất</button>
      </div>
    </header>

    <div class="selectors">
      <label>
        Dự án
        <select v-model="projectId" :disabled="projects.length === 0">
          <option v-for="project in projects" :key="project.id" :value="project.id">
            {{ project.name }}
          </option>
        </select>
      </label>
      <label>
        Sub-project
        <select v-model="repositoryId" :disabled="repositories.length === 0">
          <option v-for="repo in repositories" :key="repo.id" :value="repo.id">
            {{ repo.name }}
          </option>
        </select>
      </label>
    </div>

    <p v-if="loadError !== null" class="error" role="alert">{{ loadError }}</p>
    <p v-if="!loadError && projects.length === 0" class="muted">Chưa có dự án nào.</p>

    <div class="workspace">
      <aside class="sidebar">
        <LogTree :rows="treeRows" :selected-key="selectedKey" @open="onOpen" />
      </aside>
      <LogLines :title="fileTitle" :lines="fileLines" :status="fileStatus" :error="fileError" />
    </div>
  </main>
</template>

<style scoped>
.logs {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  height: 100vh;
  box-sizing: border-box;
  padding: 1rem 1.25rem;
}

.topbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.topbar h1 {
  margin: 0;
  font-size: 1.4rem;
}

.who {
  display: flex;
  gap: 0.75rem;
  align-items: center;
}

.selectors {
  display: flex;
  gap: 1rem;
  flex-wrap: wrap;
}

.selectors label {
  display: grid;
  gap: 0.25rem;
  font-size: 0.85rem;
}

.selectors select {
  min-width: 14rem;
  padding: 0.35rem 0.5rem;
  font: inherit;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: var(--surface);
}

.workspace {
  display: grid;
  grid-template-columns: minmax(14rem, 20rem) minmax(0, 1fr);
  gap: 1rem;
  flex: 1;
  min-height: 0;
}

.sidebar {
  display: flex;
  min-height: 0;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 0.5rem 0;
}

.sidebar > :deep(.tree) {
  flex: 1;
}

.muted {
  color: var(--muted);
}

.error {
  color: var(--danger);
}
</style>
