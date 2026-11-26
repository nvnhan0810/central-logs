<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { RouterLink } from 'vue-router';

import type { NewLogRepository, NewProject, Project } from '@/application/logs/log-gateway';
import ProjectFormDialog from '@/presentation/components/ProjectFormDialog.vue';
import RepositoryFormDialog from '@/presentation/components/RepositoryFormDialog.vue';
import { errorMessage } from '@/presentation/logs/error-message';
import { useLogGateway } from '@/presentation/logs/log-injection';
import { useProjectCatalog } from '@/presentation/projects/use-project-catalog';
import { ROUTE_NAME } from '@/presentation/router/route-names';

const catalog = useProjectCatalog(useLogGateway());
const { items, loading, loadError } = catalog;

const projectDialogOpen = ref(false);
/** The project that a new sub-project is added to; `null` when that popup is closed. */
const repositoryTarget = ref<Project | null>(null);
const saving = ref(false);
const saveError = ref<string | null>(null);

function closeDialogs(): void {
  projectDialogOpen.value = false;
  repositoryTarget.value = null;
  saveError.value = null;
}

function openProjectDialog(): void {
  saveError.value = null;
  projectDialogOpen.value = true;
}

function openRepositoryDialog(project: Project): void {
  saveError.value = null;
  repositoryTarget.value = project;
}

/** Runs one save. Returns `true` on success; on failure the message is shown in the open popup. */
async function save(action: () => Promise<void>): Promise<boolean> {
  saving.value = true;
  saveError.value = null;
  try {
    await action();
    return true;
  } catch (error: unknown) {
    saveError.value = errorMessage(error);
    return false;
  } finally {
    saving.value = false;
  }
}

async function onAddProject(input: NewProject): Promise<void> {
  if (await save(() => catalog.addProject(input))) {
    closeDialogs();
  }
}

async function onAddRepository(input: NewLogRepository): Promise<void> {
  const target = repositoryTarget.value;
  if (target === null) {
    return;
  }
  if (await save(() => catalog.addRepository(target.id, input))) {
    closeDialogs();
  }
}

onMounted(() => {
  void catalog.load();
});
</script>

<template>
  <main class="projects">
    <header class="topbar">
      <div>
        <RouterLink class="back" :to="{ name: ROUTE_NAME.home }">← Nhật ký log</RouterLink>
        <h1>Dự án và sub-project</h1>
      </div>
      <button type="button" @click="openProjectDialog">Thêm dự án</button>
    </header>

    <p v-if="loadError !== null" class="error" role="alert">{{ loadError }}</p>
    <p v-else-if="loading && items.length === 0" class="muted">Đang tải…</p>
    <p v-else-if="items.length === 0" class="muted">Chưa có dự án nào.</p>

    <section v-for="item in items" :key="item.project.id" class="project">
      <header class="project-head">
        <div>
          <h2>{{ item.project.name }}</h2>
          <p v-if="item.project.description" class="muted">{{ item.project.description }}</p>
        </div>
        <button type="button" class="secondary" @click="openRepositoryDialog(item.project)">
          Thêm sub-project
        </button>
      </header>

      <p v-if="item.repositories.length === 0" class="muted">Chưa có sub-project nào.</p>
      <ul v-else class="repositories">
        <li v-for="repo in item.repositories" :key="repo.id">
          <strong>{{ repo.name }}</strong>
          <ul class="directories">
            <li v-for="dir in repo.directories" :key="dir">
              <code>{{ dir }}</code>
            </li>
          </ul>
        </li>
      </ul>
    </section>

    <ProjectFormDialog
      :open="projectDialogOpen"
      :busy="saving"
      :error="saveError"
      @close="closeDialogs"
      @submit="onAddProject"
    />
    <RepositoryFormDialog
      :open="repositoryTarget !== null"
      :project-name="repositoryTarget?.name ?? ''"
      :busy="saving"
      :error="saveError"
      @close="closeDialogs"
      @submit="onAddRepository"
    />
  </main>
</template>

<style scoped>
.projects {
  display: grid;
  gap: 1rem;
  max-width: 56rem;
  margin: 0 auto;
  padding: 1rem 1.25rem;
  box-sizing: border-box;
}

.topbar,
.project-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
}

.topbar h1 {
  margin: 0.25rem 0 0;
  font-size: 1.4rem;
}

.back {
  font-size: 0.85rem;
}

.project {
  display: grid;
  gap: 0.5rem;
  padding: 1rem 1.25rem;
  background: #fff;
  border: 1px solid #dfe5ec;
  border-radius: 6px;
}

.project-head h2 {
  margin: 0;
  font-size: 1.1rem;
}

.project-head p {
  margin: 0.25rem 0 0;
}

.repositories,
.directories {
  margin: 0;
  padding: 0;
  list-style: none;
}

.repositories > li {
  padding: 0.5rem 0;
  border-top: 1px solid #eef1f5;
}

.directories {
  display: grid;
  gap: 0.2rem;
  margin-top: 0.25rem;
  font-size: 0.85rem;
}

.secondary {
  background: #fff;
  color: #1f2933;
}

.muted {
  color: #6b7785;
}

.error {
  color: #b42318;
}
</style>
