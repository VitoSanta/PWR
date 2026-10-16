import { vi } from 'vitest';
import { webViewVisible } from './web-view';

describe('Web view occlusion', () => {
  it('hides the child webview behind a menu, outside the viewport, and after detach', () => {
    const surface = document.createElement('div'); document.body.append(surface);
    const rect = new DOMRect(10, 10, 200, 100);
    const hit = vi.fn((): HTMLElement => surface);
    const original = document.elementFromPoint;
    Object.defineProperty(document, 'elementFromPoint', {configurable: true, value: hit});
    try {
      expect(webViewVisible(surface, rect)).toBe(true);
      hit.mockReturnValue(document.body);
      expect(webViewVisible(surface, rect)).toBe(false);
      hit.mockReturnValue(surface);
      expect(webViewVisible(surface, new DOMRect(innerWidth - 10, 10, 200, 100))).toBe(false);
      surface.remove();
      expect(webViewVisible(surface, rect)).toBe(false);
    } finally {
      Object.defineProperty(document, 'elementFromPoint', {configurable: true, value: original});
      surface.remove();
    }
  });
});
