import { writable } from 'svelte/store';

export type ApiChecksResult = {
  file_id: string;
  version: number;
  success: boolean;
  errors?: string[];
  timestamp?: string;
  file?: unknown;
  security?: unknown;
  raw?: unknown;
};

const results = writable<ApiChecksResult[]>([]);
const MAX_RESULTS = 50;

export function pushResult(entry: ApiChecksResult) {
  results.update((current) => {
    const next = [...current, entry];
    if (next.length > MAX_RESULTS) {
      next.splice(0, next.length - MAX_RESULTS);
    }
    return next;
  });
}

export function getResultsStore() {
  return { subscribe: results.subscribe };
}

export function clearResults() {
  results.set([]);
}
