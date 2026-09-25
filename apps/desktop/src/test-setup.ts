// Node 26 defines its own `localStorage` on the global object, which is
// undefined unless Node is started with --localstorage-file, and it hides the
// test environment's. Specs that exercise what the app remembers get an
// in-memory one instead.
for (const name of ['localStorage', 'sessionStorage'] as const) {
  let usable = false;
  try {
    usable = typeof globalThis[name]?.getItem === 'function';
  } catch {
    usable = false;
  }
  if (usable) continue;
  const items = new Map<string, string>();
  const storage: Storage = {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => void items.delete(key),
    setItem: (key, value) => void items.set(key, String(value)),
  };
  Object.defineProperty(globalThis, name, { configurable: true, value: storage });
}
