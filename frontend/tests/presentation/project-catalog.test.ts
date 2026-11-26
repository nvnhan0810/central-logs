import { describe, expect, it } from 'vitest';

import type {
  LogGateway,
  LogRepository,
  NewLogRepository,
  NewProject,
  Project,
} from '@/application/logs/log-gateway';
import { useProjectCatalog } from '@/presentation/projects/use-project-catalog';

/** In-memory backend for projects and sub-projects. */
class FakeCatalog {
  projects: Project[] = [];
  repositories: LogRepository[] = [];
  listFailure: string | null = null;
  createFailure: string | null = null;
  private nextId = 1;

  gateway(): LogGateway {
    return {
      listProjects: () => {
        if (this.listFailure !== null) {
          return Promise.reject(new Error(this.listFailure));
        }
        return Promise.resolve([...this.projects]);
      },
      listLogRepositories: (projectId: string) =>
        Promise.resolve(this.repositories.filter((r) => r.projectId === projectId)),
      createProject: (input: NewProject) => {
        if (this.createFailure !== null) {
          return Promise.reject(new Error(this.createFailure));
        }
        const id = `p${String(this.nextId++)}`;
        this.projects.push({ id, name: input.name, description: input.description });
        return Promise.resolve(id);
      },
      createLogRepository: (projectId: string, input: NewLogRepository) => {
        if (this.createFailure !== null) {
          return Promise.reject(new Error(this.createFailure));
        }
        const id = `r${String(this.nextId++)}`;
        this.repositories.push({
          id,
          projectId,
          name: input.name,
          directories: [...input.directories],
        });
        return Promise.resolve(id);
      },
      listEntries: () => Promise.reject(new Error('unused')),
      readChunk: () => Promise.reject(new Error('unused')),
    };
  }
}

describe('useProjectCatalog', () => {
  it('loads every project with its sub-projects', async () => {
    const fake = new FakeCatalog();
    fake.projects = [{ id: 'p1', name: 'Payments', description: '' }];
    fake.repositories = [{ id: 'r1', projectId: 'p1', name: 'api', directories: ['/var/log/api'] }];
    const catalog = useProjectCatalog(fake.gateway());

    await catalog.load();

    expect(catalog.items.value).toEqual([
      {
        project: { id: 'p1', name: 'Payments', description: '' },
        repositories: [{ id: 'r1', projectId: 'p1', name: 'api', directories: ['/var/log/api'] }],
      },
    ]);
    expect(catalog.loading.value).toBe(false);
    expect(catalog.loadError.value).toBeNull();
  });

  it('reports a load failure without dropping the last good list', async () => {
    const fake = new FakeCatalog();
    fake.projects = [{ id: 'p1', name: 'Payments', description: '' }];
    const catalog = useProjectCatalog(fake.gateway());
    await catalog.load();

    fake.listFailure = 'network down';
    await catalog.load();

    expect(catalog.loadError.value).toBe('Không tải được danh sách dự án: network down');
    expect(catalog.items.value).toHaveLength(1);
  });

  it('adds a project and shows it after reloading', async () => {
    const fake = new FakeCatalog();
    const catalog = useProjectCatalog(fake.gateway());

    await catalog.addProject({ name: 'Payments', description: 'Thanh toán' });

    expect(catalog.items.value.map((item) => item.project.name)).toEqual(['Payments']);
    expect(catalog.items.value[0]?.project.description).toBe('Thanh toán');
  });

  it('adds a sub-project under the right project only', async () => {
    const fake = new FakeCatalog();
    fake.projects = [
      { id: 'p1', name: 'Payments', description: '' },
      { id: 'p2', name: 'Search', description: '' },
    ];
    const catalog = useProjectCatalog(fake.gateway());

    await catalog.addRepository('p2', { name: 'indexer', directories: ['/srv/index'] });

    const byProject = Object.fromEntries(
      catalog.items.value.map((item) => [item.project.id, item.repositories.map((r) => r.name)]),
    );
    expect(byProject).toEqual({ p1: [], p2: ['indexer'] });
  });

  it('throws the create error so the popup can show it, and leaves the list unchanged', async () => {
    const fake = new FakeCatalog();
    fake.projects = [{ id: 'p1', name: 'Payments', description: '' }];
    const catalog = useProjectCatalog(fake.gateway());
    await catalog.load();

    fake.createFailure = 'directory does not exist: /nope';
    await expect(
      catalog.addRepository('p1', { name: 'api', directories: ['/nope'] }),
    ).rejects.toThrow('directory does not exist: /nope');
    expect(catalog.items.value[0]?.repositories).toEqual([]);
  });

  it('keeps only the newest load when two overlap', async () => {
    const replies: ((projects: Project[]) => void)[] = [];
    const gateway: LogGateway = {
      ...new FakeCatalog().gateway(),
      listProjects: () =>
        new Promise<readonly Project[]>((resolve) => {
          replies.push(resolve);
        }),
      listLogRepositories: () => Promise.resolve([]),
    };
    const catalog = useProjectCatalog(gateway);

    const older = catalog.load();
    const newer = catalog.load();
    replies[1]?.([{ id: 'new', name: 'New', description: '' }]);
    await newer;
    replies[0]?.([{ id: 'old', name: 'Old', description: '' }]);
    await older;

    expect(catalog.items.value.map((item) => item.project.id)).toEqual(['new']);
    expect(catalog.loading.value).toBe(false);
  });
});
