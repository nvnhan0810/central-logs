import { describe, expect, it } from 'vitest';

import {
  childNode,
  flattenRows,
  lastSegment,
  rootNode,
  type TreeNode,
} from '@/presentation/logs/tree-model';

describe('log tree model', () => {
  it('labels a root with the last segment of its directory', () => {
    expect(rootNode('/var/log/api/').name).toBe('api');
    expect(rootNode('/').name).toBe('/');
    expect(lastSegment('/srv/app')).toBe('app');
  });

  it('shows only the roots while nothing is expanded', () => {
    const roots = [rootNode('/var/log/api'), rootNode('/srv/app/logs')];
    expect(flattenRows(roots).map((r) => r.node.name)).toEqual(['api', 'logs']);
  });

  it('shows children under an expanded directory, indented one level deeper', () => {
    const root = rootNode('/var/log/api');
    root.expanded = true;
    root.children = [
      childNode('/var/log/api', {
        name: '2026',
        relativePath: '2026',
        kind: 'directory',
        size: null,
      }),
      childNode('/var/log/api', {
        name: 'app.log',
        relativePath: 'app.log',
        kind: 'file',
        size: 10,
      }),
    ];
    const rows = flattenRows([root]);
    expect(rows.map((r) => [r.node.name, r.depth])).toEqual([
      ['api', 0],
      ['2026', 1],
      ['app.log', 1],
    ]);
  });

  it('keeps a collapsed directory children out of the rows', () => {
    const root: TreeNode = rootNode('/var/log/api');
    root.children = [
      childNode('/var/log/api', { name: 'a.log', relativePath: 'a.log', kind: 'file', size: 1 }),
    ];
    expect(flattenRows([root])).toHaveLength(1);
  });
});
