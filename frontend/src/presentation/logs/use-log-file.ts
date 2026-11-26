import { onScopeDispose, ref, shallowRef, type Ref, type ShallowRef } from 'vue';

import type { LogGateway, LogLine } from '@/application/logs/log-gateway';
import { readFrom, type FileTarget } from '@/application/logs/read-from';

import { errorMessage } from './error-message';

export type FileStatus = 'idle' | 'loading' | 'live' | 'error';

export interface UseLogFile {
  readonly target: ShallowRef<FileTarget | null>;
  /** Every line read so far, oldest first. */
  readonly lines: ShallowRef<readonly LogLine[]>;
  readonly status: Ref<FileStatus>;
  readonly error: Ref<string | null>;
  /** Opens a file, reads it to the end, then keeps reading the newly appended content. */
  open(next: FileTarget): Promise<void>;
  close(): void;
}

/**
 * Follows one log file. The backend has no push, so new content is fetched by polling
 * from the last offset read. Only bytes past that offset are transferred.
 */
export function useLogFile(gateway: LogGateway, pollIntervalMs: number): UseLogFile {
  const target = shallowRef<FileTarget | null>(null);
  const lines = shallowRef<readonly LogLine[]>([]);
  const status = ref<FileStatus>('idle');
  const error = ref<string | null>(null);

  let generation = 0;
  let offset = 0;
  let busy = false;
  // After a failed poll the file may have shrunk or rotated, so the next poll starts again from the top.
  let needsReset = false;
  let timer: ReturnType<typeof setInterval> | null = null;

  function stopTimer(): void {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  async function refresh(): Promise<void> {
    const current = target.value;
    if (current === null || busy) {
      return;
    }
    const gen = generation;
    const reset = needsReset;
    busy = true;
    try {
      const result = await readFrom(gateway, current, reset ? 0 : offset, () => gen === generation);
      if (result === null || gen !== generation) {
        return;
      }
      lines.value = reset ? result.lines : [...lines.value, ...result.lines];
      offset = result.nextOffset;
      needsReset = false;
      error.value = null;
      status.value = 'live';
    } catch (cause: unknown) {
      if (gen === generation) {
        needsReset = true;
        error.value = errorMessage(cause);
        status.value = 'error';
      }
    } finally {
      if (gen === generation) {
        busy = false;
      }
    }
  }

  async function open(next: FileTarget): Promise<void> {
    stopTimer();
    generation++;
    busy = false;
    target.value = next;
    lines.value = [];
    offset = 0;
    needsReset = false;
    error.value = null;
    status.value = 'loading';

    const gen = generation;
    await refresh();
    if (gen === generation) {
      timer = setInterval(() => {
        void refresh();
      }, pollIntervalMs);
    }
  }

  function close(): void {
    stopTimer();
    generation++;
    busy = false;
    target.value = null;
    lines.value = [];
    offset = 0;
    needsReset = false;
    error.value = null;
    status.value = 'idle';
  }

  onScopeDispose(close);

  return { target, lines, status, error, open, close };
}
