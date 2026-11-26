import { computed, ref, type ComputedRef } from 'vue';

import type { LogGateway, LogRepository } from '@/application/logs/log-gateway';

import { errorMessage } from './error-message';
import { childNode, flattenRows, rootNode, type TreeNode, type TreeRow } from './tree-model';

export interface UseLogTree {
  /** Visible rows of the tree, top to bottom. */
  readonly rows: ComputedRef<TreeRow[]>;
  /** Shows the directories of `repository` as roots (or nothing). */
  show(repository: LogRepository | null): void;
  /** Expands or collapses a directory. The children are fetched the first time it opens. */
  toggle(node: TreeNode): Promise<void>;
}

/** Lazily loaded directory tree of one sub-project. Files are not toggled; the caller opens them. */
export function useLogTree(gateway: LogGateway): UseLogTree {
  const roots = ref<TreeNode[]>([]);
  const rows = computed(() => flattenRows(roots.value));
  let repositoryId: string | null = null;
  // Responses for an older repository must not land in the current tree.
  let generation = 0;

  function show(repository: LogRepository | null): void {
    generation++;
    repositoryId = repository?.id ?? null;
    roots.value = repository === null ? [] : repository.directories.map((dir) => rootNode(dir));
  }

  async function toggle(node: TreeNode): Promise<void> {
    if (node.kind !== 'directory') {
      return;
    }
    if (node.expanded) {
      node.expanded = false;
      return;
    }
    node.expanded = true;
    if (node.children !== null || repositoryId === null) {
      return;
    }

    const gen = generation;
    const repository = repositoryId;
    node.loading = true;
    node.error = null;
    try {
      const entries = await gateway.listEntries(repository, node.directory, node.relativePath);
      if (gen === generation) {
        node.children = entries.map((entry) => childNode(node.directory, entry));
      }
    } catch (error: unknown) {
      if (gen === generation) {
        node.error = errorMessage(error);
      }
    } finally {
      node.loading = false;
    }
  }

  return { rows, show, toggle };
}
