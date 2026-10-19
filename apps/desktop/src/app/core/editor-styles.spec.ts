import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => {
  document.querySelectorAll('link[data-pwr-editor-styles]').forEach(link => link.remove());
  vi.resetModules();
});

describe('local editor styles', () => {
  it('shares a single load and waits for the stylesheet before resolving', async () => {
    const { loadEditorStyles } = await import('./editor-styles');
    const first = loadEditorStyles();
    expect(loadEditorStyles()).toBe(first);
    const links = document.querySelectorAll<HTMLLinkElement>('link[data-pwr-editor-styles]');
    expect(links).toHaveLength(1);
    expect(links[0].href).toBe(new URL('monaco/editor.main.css', document.baseURI).href);
    let ready = false;
    void first.then(() => ready = true);
    await Promise.resolve();
    expect(ready).toBe(false);
    links[0].dispatchEvent(new Event('load'));
    await first;
    expect(ready).toBe(true);
    expect(loadEditorStyles()).toBe(first);
  });

  it('removes a failed stylesheet and lets reopening retry', async () => {
    const { loadEditorStyles } = await import('./editor-styles');
    const failed = loadEditorStyles();
    const rejected = expect(failed).rejects.toThrow('local editor stylesheet');
    document.querySelector('link[data-pwr-editor-styles]')!.dispatchEvent(new Event('error'));
    await rejected;
    expect(document.querySelector('link[data-pwr-editor-styles]')).toBeNull();
    const retry = loadEditorStyles();
    expect(retry).not.toBe(failed);
    document.querySelector('link[data-pwr-editor-styles]')!.dispatchEvent(new Event('load'));
    await retry;
  });
});
