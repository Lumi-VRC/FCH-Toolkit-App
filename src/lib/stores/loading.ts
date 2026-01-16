// Loading state store - tracks app initialization
import { writable } from 'svelte/store';

export const isLoading = writable(true);
export const loadingMessage = writable('Initializing...');

// Mark a specific initialization task as complete
export function completeTask(taskName: string) {
  console.log(`[Loading] Completed: ${taskName}`);
}

// Set loading state
export function setLoading(loading: boolean, message?: string) {
  isLoading.set(loading);
  if (message) {
    loadingMessage.set(message);
  }
}
