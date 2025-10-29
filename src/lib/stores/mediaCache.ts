const resolved = new Map<string, string>();
const pending = new Map<string, Promise<string>>();
const MAX_CONCURRENT_PREFETCH = 4;
const inProgress = new Set<string>();

async function fetchAsObjectUrl(url: string): Promise<string> {
  try {
    const response = await fetch(url, { cache: 'force-cache' });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const blob = await response.blob();
    return URL.createObjectURL(blob);
  } catch (err) {
    console.warn('[mediaCache] falling back to original URL', url, err);
    return url;
  }
}

export function getMediaSource(url: string): Promise<string> {
  if (!url) return Promise.resolve('');

  const cached = resolved.get(url);
  if (cached) return Promise.resolve(cached);

  const inflight = pending.get(url);
  if (inflight) return inflight;

  const promise = fetchAsObjectUrl(url)
    .then((objectUrl) => {
      resolved.set(url, objectUrl);
      pending.delete(url);
      return objectUrl;
    })
    .catch((err) => {
      pending.delete(url);
      throw err;
    })
    .finally(() => {
      inProgress.delete(url);
    });

  pending.set(url, promise);
  return promise;
}

export function prefetchMedia(urls: Iterable<string>) {
  for (const url of urls) {
    if (inProgress.size >= MAX_CONCURRENT_PREFETCH) break;
    if (resolved.has(url) || pending.has(url) || inProgress.has(url) || !url) continue;
    inProgress.add(url);
    console.log('[mediaCache] prefetching', url);
    void getMediaSource(url).catch((err) => {
      console.warn('[mediaCache] prefetch failed', url, err);
    });
  }
}

export function clearMediaCache() {
  for (const objectUrl of resolved.values()) {
    if (objectUrl.startsWith('blob:')) {
      URL.revokeObjectURL(objectUrl);
    }
  }
  resolved.clear();
  pending.clear();
}

