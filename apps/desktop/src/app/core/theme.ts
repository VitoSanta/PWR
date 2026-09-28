import { Injectable, computed, effect, signal } from '@angular/core';
import { inTauri } from './bridge';
import { PALETTES } from './palettes';

export type ThemeMode = 'system' | 'light' | 'dark';
export type Theme = 'light' | 'dark';

/** Where the choice is kept; `index.html` reads the same key before first paint. */
export const THEME_KEY = 'pwr:theme';

export function readThemeMode(
  storage: Pick<Storage, 'getItem'> | undefined,
  search = '',
): ThemeMode {
  // A palette named in the address shows in its own scheme.
  const asked = new URLSearchParams(search).get('palette');
  const scheme = PALETTES.find((palette) => palette.id === asked)?.scheme;
  if (scheme) return scheme;
  try {
    const saved = storage?.getItem(THEME_KEY);
    return saved === 'light' || saved === 'dark' ? saved : 'system';
  } catch {
    return 'system';
  }
}

export function resolveTheme(mode: ThemeMode, systemDark: boolean): Theme {
  return mode === 'system' ? (systemDark ? 'dark' : 'light') : mode;
}

/**
 * The colour theme used when dark, and the one when light -- as the editor
 * does it, so System can switch between two chosen palettes. `layout` means
 * the layout's own colours. `index.html` reads the same keys before first paint.
 */
export const PALETTE_KEYS: Record<Theme, string> = {
  dark: 'pwr:palette-dark',
  light: 'pwr:palette-light',
};
export const LAYOUT_PALETTE = 'layout';

export function isPalette(id: string | null | undefined, scheme: Theme): id is string {
  return PALETTES.some((palette) => palette.id === id && palette.scheme === scheme);
}

export function readPalette(
  storage: Pick<Storage, 'getItem'> | undefined,
  scheme: Theme,
  search = '',
): string {
  // `?palette=midnight` wins, so a palette can be opened in a plain browser.
  const asked = new URLSearchParams(search).get('palette');
  if (isPalette(asked, scheme)) return asked;
  try {
    const saved = storage?.getItem(PALETTE_KEYS[scheme]);
    return isPalette(saved, scheme) ? saved : LAYOUT_PALETTE;
  } catch {
    return LAYOUT_PALETTE;
  }
}

/**
 * System, Light or Dark. System follows the operating system live; an
 * explicit choice overrides it and is remembered.
 */
@Injectable({ providedIn: 'root' })
export class ThemeService {
  private readonly media =
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-color-scheme: dark)')
      : null;
  private readonly storage = typeof localStorage !== 'undefined' ? localStorage : undefined;

  private readonly search = typeof location !== 'undefined' ? location.search : '';
  readonly mode = signal<ThemeMode>(readThemeMode(this.storage, this.search));
  private readonly systemDark = signal(this.media?.matches ?? true);
  readonly theme = computed(() => resolveTheme(this.mode(), this.systemDark()));
  readonly darkPalette = signal(readPalette(this.storage, 'dark', this.search));
  readonly lightPalette = signal(readPalette(this.storage, 'light', this.search));
  /** The palette in use now, or `layout` for the layout's own colours. */
  readonly palette = computed(() =>
    this.theme() === 'dark' ? this.darkPalette() : this.lightPalette(),
  );

  constructor() {
    this.media?.addEventListener('change', (event) => this.systemDark.set(event.matches));

    effect(() => {
      const root = typeof document !== 'undefined' ? document.documentElement : null;
      if (!root) return;
      root.dataset['theme'] = this.theme();
      const palette = this.palette();
      if (palette === LAYOUT_PALETTE) delete root.dataset['palette'];
      else root.dataset['palette'] = palette;
    });

    effect(() => {
      const chosen: Record<Theme, string> = {
        dark: this.darkPalette(),
        light: this.lightPalette(),
      };
      for (const scheme of ['dark', 'light'] as Theme[]) {
        try {
          if (chosen[scheme] === LAYOUT_PALETTE) this.storage?.removeItem(PALETTE_KEYS[scheme]);
          else this.storage?.setItem(PALETTE_KEYS[scheme], chosen[scheme]);
        } catch {
          /* storage unavailable: the choice lasts for this run */
        }
      }
    });

    effect(() => {
      const mode = this.mode();
      try {
        if (mode === 'system') this.storage?.removeItem(THEME_KEY);
        else this.storage?.setItem(THEME_KEY, mode);
      } catch {
        /* storage unavailable: the choice lasts for this run */
      }
      // The native window follows too, so the title bar and traffic lights
      // match; null hands it back to the operating system.
      if (inTauri()) {
        void import('@tauri-apps/api/window')
          .then(({ getCurrentWindow }) =>
            getCurrentWindow().setTheme(mode === 'system' ? null : mode),
          )
          .catch(() => undefined);
      }
    });
  }

  set(mode: ThemeMode): void {
    this.mode.set(mode);
  }

  /** The palette to use when `scheme` is showing; `layout` for the layout's own colours. */
  setPalette(scheme: Theme, id: string): void {
    if (id !== LAYOUT_PALETTE && !isPalette(id, scheme)) return;
    (scheme === 'dark' ? this.darkPalette : this.lightPalette).set(id);
  }
}
