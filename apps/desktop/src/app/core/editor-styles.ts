/** Load Monaco's complete local stylesheet before creating any editor. */
let pending: Promise<void> | undefined;

export function loadEditorStyles(): Promise<void> {
  if (pending) return pending;
  pending = new Promise<void>((resolve, reject) => {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = new URL('monaco/editor.main.css', document.baseURI).href;
    link.dataset['pwrEditorStyles'] = '';
    link.onload = () => resolve();
    link.onerror = () => {
      link.remove();
      pending = undefined;
      reject(new Error('The local editor stylesheet could not load. Reopen the editor to retry.'));
    };
    document.head.appendChild(link);
  });
  return pending;
}
