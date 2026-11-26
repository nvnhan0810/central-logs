import type { LogEntry } from '@/application/logs/log-gateway';

/** One node of the log tree. A directory node may hold its loaded children. */
export interface TreeNode {
  readonly key: string;
  readonly name: string;
  readonly kind: 'file' | 'directory';
  /** Registered absolute directory the node belongs to. */
  readonly directory: string;
  /** Path below `directory`; empty for the directory root itself. */
  readonly relativePath: string;
  expanded: boolean;
  loading: boolean;
  error: string | null;
  /** `null` until the children are fetched for the first time. */
  children: TreeNode[] | null;
}

export interface TreeRow {
  readonly node: TreeNode;
  readonly depth: number;
}

export function nodeKey(directory: string, relativePath: string): string {
  return `${directory}\u0000${relativePath}`;
}

function createNode(
  name: string,
  kind: 'file' | 'directory',
  directory: string,
  relativePath: string,
): TreeNode {
  return {
    key: nodeKey(directory, relativePath),
    name,
    kind,
    directory,
    relativePath,
    expanded: false,
    loading: false,
    error: null,
    children: null,
  };
}

/** Last path segment, used as the label of a registered directory. */
export function lastSegment(path: string): string {
  const segment = path.replace(/\/+$/, '').split('/').pop();
  return segment === undefined || segment === '' ? path : segment;
}

export function rootNode(directory: string): TreeNode {
  return createNode(lastSegment(directory), 'directory', directory, '');
}

export function childNode(directory: string, entry: LogEntry): TreeNode {
  return createNode(entry.name, entry.kind, directory, entry.relativePath);
}

/** Visible rows in display order: a node, then its children when it is expanded. */
export function flattenRows(nodes: readonly TreeNode[], depth = 0): TreeRow[] {
  const rows: TreeRow[] = [];
  for (const node of nodes) {
    rows.push({ node, depth });
    if (node.expanded && node.children !== null) {
      for (const row of flattenRows(node.children, depth + 1)) {
        rows.push(row);
      }
    }
  }
  return rows;
}
