import { Injectable, computed, effect, signal } from '@angular/core';
import { inTauri } from './bridge';

export type ThemeMode = 'system' | 'light' | 'dark';
export type Theme = 'light' | 'dark';

/** Where the choice is kept; `index.html` reads the same key before first paint. */
export const THEME_KEY = 'pwr:theme';

export function readThemeMode(storage: Pick<Storage, 'getItem'> | undefined): ThemeMode {
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

  readonly mode = signal<ThemeMode>(readThemeMode(this.storage));
  private readonly systemDark = signal(this.media?.matches ?? true);
  readonly theme = computed(() => resolveTheme(this.mode(), this.systemDark()));

  constructor() {
    this.media?.addEventListener('change', (event) => this.systemDark.set(event.matches));

    effect(() => {
      const root = typeof document !== 'undefined' ? document.documentElement : null;
      if (root) root.dataset['theme'] = this.theme();
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
}
