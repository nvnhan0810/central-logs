import { ref, type Ref } from 'vue';

import type {
  LogGateway,
  LogRepository,
  NewLogRepository,
  NewProject,
  Project,
} from '@/application/logs/log-gateway';
import { errorMessage } from '@/presentation/logs/error-message';

/** A project with all of its sub-projects. */
export interface CatalogProject {
  readonly project: Project;
  readonly repositories: readonly LogRepository[];
}

export interface ProjectCatalog {
  readonly items: Ref<readonly CatalogProject[]>;
  readonly loading: Ref<boolean>;
  /** Set when the list could not be loaded; `null` otherwise. */
  readonly loadError: Ref<string | null>;
  /** Loads every project and its sub-projects. Failures are shown in `loadError`. */
  load(): Promise<void>;
  /** Creates a project, then reloads. Rejects with the backend's message when the create fails. */
  addProject(input: NewProject): Promise<void>;
  /** Creates a sub-project, then reloads. Rejects with the backend's message when the create fails. */
  addRepository(projectId: string, input: NewLogRepository): Promise<void>;
}

/** State and actions of the projects page. Errors from writes are thrown so the dialog can show them. */
export function useProjectCatalog(gateway: LogGateway): ProjectCatalog {
  const items = ref<CatalogProject[]>([]);
  const loading = ref(false);
  const loadError = ref<string | null>(null);
  // Only the newest load may write its result, so a slow older response cannot overwrite a newer one.
  let generation = 0;

  async function fetchAll(): Promise<CatalogProject[]> {
    const projects = await gateway.listProjects();
    return Promise.all(
      projects.map(async (project) => ({
        project,
        repositories: await gateway.listLogRepositories(project.id),
      })),
    );
  }

  async function load(): Promise<void> {
    const gen = ++generation;
    loading.value = true;
    try {
      const loaded = await fetchAll();
      if (gen === generation) {
        items.value = loaded;
        loadError.value = null;
      }
    } catch (error: unknown) {
      if (gen === generation) {
        loadError.value = `Không tải được danh sách dự án: ${errorMessage(error)}`;
      }
    } finally {
      if (gen === generation) {
        loading.value = false;
      }
    }
  }

  async function addProject(input: NewProject): Promise<void> {
    await gateway.createProject(input);
    await load();
  }

  async function addRepository(projectId: string, input: NewLogRepository): Promise<void> {
    await gateway.createLogRepository(projectId, input);
    await load();
  }

  return { items, loading, loadError, load, addProject, addRepository };
}
