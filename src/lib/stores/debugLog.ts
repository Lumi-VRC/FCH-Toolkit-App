import { writable } from 'svelte/store';

export type DebugEntry = {
  ts: string;
  message: string;
};

const MAX_ENTRIES = 100;

const { subscribe, update, set } = writable<DebugEntry[]>([]);
const avatarSwitchCounter = writable(0);
const apiCallSentCounter = writable(0);
const apiCallReturnCounter = writable(0);
const apiQueueLengthStore = writable(0);
const liveViewCountsStore = writable({ loaded: 0, total: 0 });

export const debugLog = { subscribe };
export const avatarSwitchCount = avatarSwitchCounter;
export const apiCallSentCount = apiCallSentCounter;
export const apiCallReturnCount = apiCallReturnCounter;
export const apiQueueLength = apiQueueLengthStore;
export const liveViewCounts = liveViewCountsStore;

export function pushDebug(message: string, ts?: string) {
  if (!message) return;
  const stamp = ts ?? new Date().toISOString();
  update((entries) => {
    const next = [...entries, { ts: stamp, message }];
    if (next.length > MAX_ENTRIES) {
      return next.slice(next.length - MAX_ENTRIES);
    }
    return next;
  });

  if (message.includes('[watcher] avatar log inserted')) {
    avatarSwitchCounter.update((n) => n + 1);
  }
  if (message.includes('[apiChecks] processing job')) {
    apiCallSentCounter.update((n) => n + 1);
  }
  if (message.includes('[VRCAPI] security-check complete')) {
    apiCallReturnCounter.update((n) => n + 1);
  }
}

export function setApiQueueLength(len: number) {
  const sanitized = Number.isFinite(len) ? Math.max(0, Math.floor(len)) : 0;
  apiQueueLengthStore.set(sanitized);
}

export function setLiveViewCounts(loaded: number, total: number) {
  const safeLoaded = Number.isFinite(loaded) ? Math.max(0, Math.floor(loaded)) : 0;
  const safeTotal = Number.isFinite(total) ? Math.max(0, Math.floor(total)) : 0;
  liveViewCountsStore.set({ loaded: safeLoaded, total: safeTotal });
}

export function clearDebugLog() {
  set([]);
}

