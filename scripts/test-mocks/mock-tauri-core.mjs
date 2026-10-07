// Mock for @tauri-apps/api/core: programmable invoke().
// The test sets per-command handlers (values, deferred promises, rejections)
// to simulate native timing: slow commands, late responses, failures.
const handlers = new Map();

export function __mockInvoke(cmd, fn) {
  handlers.set(cmd, fn);
}

export function __resetInvoke() {
  handlers.clear();
}

export function invoke(cmd, args) {
  const fn = handlers.get(cmd);
  if (!fn) {
    return Promise.reject(new Error(`no mock for invoke("${cmd}")`));
  }
  try {
    return Promise.resolve(fn(args));
  } catch (err) {
    return Promise.reject(err);
  }
}
