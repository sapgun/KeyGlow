// Mock for @tauri-apps/api/event: captures listen() handlers so the test
// can fire native events with chosen payloads and timing, simulating
// late/duplicate/reordered delivery.
const listeners = new Map();

export function __resetEvents() {
  listeners.clear();
}

/** Simulate a native event arriving with the given payload. */
export function __fire(event, payload) {
  const handlers = listeners.get(event) ?? [];
  for (const handler of [...handlers]) {
    handler({ payload });
  }
}

export function __listenerCount(event) {
  return (listeners.get(event) ?? []).length;
}

export function listen(event, handler) {
  if (!listeners.has(event)) listeners.set(event, []);
  listeners.get(event).push(handler);
  return Promise.resolve(() => {
    const arr = listeners.get(event) ?? [];
    const i = arr.indexOf(handler);
    if (i >= 0) arr.splice(i, 1);
  });
}
