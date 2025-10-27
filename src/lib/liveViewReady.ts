type Deferred = {
  promise: Promise<void>;
  resolve: () => void;
  resolved: boolean;
};

const GLOBAL_KEY = '__FCH_LIVE_VIEW_READY__';

function getDeferred(): Deferred {
  const globalObj = globalThis as typeof globalThis & {
    [GLOBAL_KEY]?: Deferred;
  };

  let deferred = globalObj[GLOBAL_KEY];
  if (!deferred) {
    let resolveFn: () => void = () => {};
    const promise = new Promise<void>((resolve) => {
      resolveFn = () => {
        if (!deferred?.resolved) {
          deferred!.resolved = true;
          resolve();
        }
      };
    });
    deferred = {
      promise,
      resolve: () => resolveFn(),
      resolved: false
    };
    globalObj[GLOBAL_KEY] = deferred;
  }
  return deferred;
}

export function markLiveViewListenersReady(): void {
  const deferred = getDeferred();
  if (!deferred.resolved) {
    deferred.resolve();
  }
}

export async function waitForLiveViewListenersReady(timeoutMs = 2000): Promise<void> {
  const deferred = getDeferred();
  if (deferred.resolved) return;

  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
    await deferred.promise;
    return;
  }

  await Promise.race([
    deferred.promise,
    new Promise<void>((resolve) => {
      setTimeout(resolve, timeoutMs);
    })
  ]);
}

