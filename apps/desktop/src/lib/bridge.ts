// One door to the backend. Inside Tauri it calls the Rust commands; in a
// plain browser (dev, screenshots, a future landing page) it loads the mock.

type Unlisten = () => void;
type Backend = {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, cb: (payload: T) => void): Promise<Unlisten>;
};

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let backend: Promise<Backend> | undefined;

function load(): Promise<Backend> {
  backend ??= isTauri
    ? Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]).then(([core, event]) => ({
        invoke: (cmd, args) => core.invoke(cmd, args),
        listen: (name, cb) => event.listen(name, (e) => cb(e.payload as never)),
      }))
    : import("@/mock/backend").then((m) => m.mockBackend);
  return backend;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return (await load()).invoke<T>(cmd, args);
}

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<Unlisten> {
  return (await load()).listen<T>(event, cb);
}
