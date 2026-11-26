<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { RouterLink } from 'vue-router';

import type {
  LogRepository,
  NewLogRepository,
  NewProject,
  Project,
} from '@/application/logs/log-gateway';
import ConfirmDialog from '@/presentation/components/ConfirmDialog.vue';
import ProjectFormDialog from '@/presentation/components/ProjectFormDialog.vue';
import RepositoryFormDialog from '@/presentation/components/RepositoryFormDialog.vue';
import { errorMessage } from '@/presentation/logs/error-message';
import { useLogGateway } from '@/presentation/logs/log-injection';
import { useProjectCatalog } from '@/presentation/projects/use-project-catalog';
import { ROUTE_NAME } from '@/presentation/router/route-names';

const catalog = useProjectCatalog(useLogGateway());
const { items, loading, loadError } = catalog;

/** Project popup: open with `editingProject` `null` to add, or set to the project being edited. */
const projectDialogOpen = ref(false);
const editingProject = ref<Project | null>(null);
/** Sub-project popup: `project` is the parent; `repository` is `null` when adding. */
const repositoryDialog = ref<{ project: Project; repository: LogRepository | null } | null>(null);
/** What the delete popup will remove. */
type DeleteTarget =
  | { kind: 'project'; project: Project; subProjectCount: number }
  | { kind: 'repository'; repository: LogRepository };

/** Delete confirmation; `null` when that popup is closed. */
const pendingDelete = ref<DeleteTarget | null>(null);

const saving = ref(false);
const saveError = ref<string | null>(null);

const deleteTitle = computed(() =>
  pendingDelete.value?.kind === 'project' ? 'Xóa dự án' : 'Xóa sub-project',
);
const deleteMessage = computed(() => {
  const target = pendingDelete.value;
  if (target === null) {
    return '';
  }
  if (target.kind === 'project') {
    const count = target.subProjectCount;
    const extra = count === 0 ? '' : ` Cùng với ${String(count)} sub-project của dự án này.`;
    return `Xóa dự án "${target.project.name}"?${extra} Thao tác này không hoàn tác được.`;
  }
  return `Xóa sub-project "${target.repository.name}"? Thao tác này không hoàn tác được.`;
});

function closeDialogs(): void {
  projectDialogOpen.value = false;
  repositoryDialog.value = null;
  pendingDelete.value = null;
  saveError.value = null;
}

function openProjectDialog(project: Project | null = null): void {
  saveError.value = null;
  editingProject.value = project;
  projectDialogOpen.value = true;
}

function openRepositoryDialog(project: Project, repository: LogRepository | null = null): void {
  saveError.value = null;
  repositoryDialog.value = { project, repository };
}

function openDelete(target: DeleteTarget): void {
  saveError.value = null;
  pendingDelete.value = target;
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

async function onSubmitProject(input: NewProject): Promise<void> {
  const project = editingProject.value;
  const done = await save(() =>
    project === null ? catalog.addProject(input) : catalog.updateProject(project.id, input),
  );
  if (done) {
    closeDialogs();
  }
}

async function onSubmitRepository(input: NewLogRepository): Promise<void> {
  const target = repositoryDialog.value;
  if (target === null) {
    return;
  }
  const done = await save(() =>
    target.repository === null
      ? catalog.addRepository(target.project.id, input)
      : catalog.updateRepository(target.repository.id, input),
  );
  if (done) {
    closeDialogs();
  }
}

async function onConfirmDelete(): Promise<void> {
  const target = pendingDelete.value;
  if (target === null) {
    return;
  }
  const done = await save(() =>
    target.kind === 'project'
      ? catalog.deleteProject(target.project.id)
      : catalog.deleteRepository(target.repository.id),
  );
  if (done) {
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
      <button type="button" @click="openProjectDialog()">Thêm dự án</button>
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
        <div class="row-actions">
          <button type="button" class="secondary" @click="openRepositoryDialog(item.project)">
            Thêm sub-project
          </button>
          <button type="button" class="secondary" @click="openProjectDialog(item.project)">
            Sửa
          </button>
          <button
            type="button"
            class="danger-outline"
            @click="
              openDelete({
                kind: 'project',
                project: item.project,
                subProjectCount: item.repositories.length,
              })
            "
          >
            Xóa
          </button>
        </div>
      </header>

      <p v-if="item.repositories.length === 0" class="muted">Chưa có sub-project nào.</p>
      <ul v-else class="repositories">
        <li v-for="repo in item.repositories" :key="repo.id">
          <div class="repo-head">
            <strong>{{ repo.name }}</strong>
            <div class="row-actions">
              <button
                type="button"
                class="secondary small"
                @click="openRepositoryDialog(item.project, repo)"
              >
                Sửa
              </button>
              <button
                type="button"
                class="danger-outline small"
                @click="openDelete({ kind: 'repository', repository: repo })"
              >
                Xóa
              </button>
            </div>
          </div>
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
      :initial="editingProject"
      :busy="saving"
      :error="saveError"
      @close="closeDialogs"
      @submit="onSubmitProject"
    />
    <RepositoryFormDialog
      :open="repositoryDialog !== null"
      :project-name="repositoryDialog?.project.name ?? ''"
      :initial="repositoryDialog?.repository ?? null"
      :busy="saving"
      :error="saveError"
      @close="closeDialogs"
      @submit="onSubmitRepository"
    />
    <ConfirmDialog
      :open="pendingDelete !== null"
      :title="deleteTitle"
      :message="deleteMessage"
      :busy="saving"
      :error="saveError"
      @close="closeDialogs"
      @confirm="onConfirmDelete"
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
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 6px;
}

.project-head h2 {
  margin: 0;
  font-size: 1.1rem;
}

.project-head p {
  margin: 0.25rem 0 0;
}

.row-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.repo-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
}

.repositories,
.directories {
  margin: 0;
  padding: 0;
  list-style: none;
}

.repositories > li {
  padding: 0.5rem 0;
  border-top: 1px solid var(--border);
}

.directories {
  display: grid;
  gap: 0.2rem;
  margin-top: 0.25rem;
  font-size: 0.85rem;
}

.secondary {
  background: var(--surface);
  color: var(--text);
}

.small {
  padding: 0.3rem 0.6rem;
  font-size: 0.85rem;
}

.danger-outline {
  background: var(--surface);
  color: var(--danger);
  border-color: var(--danger);
}

.muted {
  color: var(--muted);
}

.error {
  color: var(--danger);
}
</style>
