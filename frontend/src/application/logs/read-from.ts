import type { FileChunk, LogGateway, LogLine } from './log-gateway';

/** Lines and bytes requested per backend call. The backend caps them at 5000 lines and 4 MiB. */
export const PAGE_MAX_LINES = 5000;
export const PAGE_MAX_BYTES = 4 * 1024 * 1024;

export interface FileTarget {
  readonly repositoryId: string;
  readonly directory: string;
  readonly relativePath: string;
}

export interface ReadResult {
  /** Lines read in file order (oldest first). */
  readonly lines: readonly LogLine[];
  /** Offset to pass next time to receive only newer content. */
  readonly nextOffset: number;
  /** File size reported by the last page. */
  readonly size: number;
  readonly modifiedUnix: number | null;
}

/**
 * Reads a file from `offset` to the current end, one page at a time, so memory stays bounded per call.
 * Returns `null` when `shouldContinue` turns false (the user moved to another file), so stale work is dropped.
 */
export async function readFrom(
  gateway: LogGateway,
  target: FileTarget,
  offset: number,
  shouldContinue: () => boolean = () => true,
): Promise<ReadResult | null> {
  const lines: LogLine[] = [];
  let cursor = offset;

  for (;;) {
    const chunk: FileChunk = await gateway.readChunk(
      target.repositoryId,
      target.directory,
      target.relativePath,
      cursor,
    );
    if (!shouldContinue()) {
      return null;
    }
    lines.push(...chunk.lines);
    // A page that makes no progress would loop forever; the backend always advances unless at EOF.
    if (chunk.nextOffset <= cursor && !chunk.eof) {
      throw new Error('Log paging did not advance');
    }
    cursor = chunk.nextOffset;
    if (chunk.eof) {
      return {
        lines,
        nextOffset: chunk.nextOffset,
        size: chunk.size,
        modifiedUnix: chunk.modifiedUnix,
      };
    }
  }
}
