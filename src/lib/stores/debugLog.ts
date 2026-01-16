import { writable } from 'svelte/store';

export type DebugEntry = {
  ts: string;
  message: string;
  level: 'log' | 'debug' | 'info' | 'warn' | 'error';
  source: 'frontend' | 'backend';
};

const MAX_ENTRIES = 500; // Increased for more comprehensive logging

const { subscribe, update, set } = writable<DebugEntry[]>([]);

export const debugLog = { subscribe };

export function pushDebug(message: string, ts?: string, level: DebugEntry['level'] = 'log', source: DebugEntry['source'] = 'frontend') {
  if (!message) return;
  const stamp = ts ?? new Date().toISOString();
  update((entries) => {
    const next = [...entries, { ts: stamp, message, level, source }];
    if (next.length > MAX_ENTRIES) {
      return next.slice(next.length - MAX_ENTRIES);
    }
    return next;
  });
}

export function clearDebugLog() {
  set([]);
}

// Intercept console methods to capture logs
// NOTE: This works in production because vite.config.js is configured to preserve console statements
// (esbuild.drop: [] ensures console.log/debug/etc are not stripped during build)
if (typeof window !== 'undefined') {
  const originalConsole = {
    log: console.log.bind(console),
    debug: console.debug.bind(console),
    info: console.info.bind(console),
    warn: console.warn.bind(console),
    error: console.error.bind(console),
  };

  // Only show console output in browser dev tools, not in Tauri console window
  // Check if we're in a Tauri environment
  const isTauri = typeof window !== 'undefined' && (window as any).__TAURI__ !== undefined;

  console.log = (...args: any[]) => {
    if (!isTauri) {
      originalConsole.log(...args);
    }
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');
    pushDebug(message, undefined, 'log', 'frontend');
  };

  console.debug = (...args: any[]) => {
    if (!isTauri) {
      originalConsole.debug(...args);
    }
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');
    pushDebug(message, undefined, 'debug', 'frontend');
  };

  console.info = (...args: any[]) => {
    if (!isTauri) {
      originalConsole.info(...args);
    }
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');
    pushDebug(message, undefined, 'info', 'frontend');
  };

  console.warn = (...args: any[]) => {
    if (!isTauri) {
      originalConsole.warn(...args);
    }
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');
    pushDebug(message, undefined, 'warn', 'frontend');
  };

  console.error = (...args: any[]) => {
    if (!isTauri) {
      originalConsole.error(...args);
    }
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');
    pushDebug(message, undefined, 'error', 'frontend');
  };
}
