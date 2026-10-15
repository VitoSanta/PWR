import { LAYOUT_SWATCHES, PALETTES } from './palettes';
import {
  LAYOUT_PALETTE,
  PALETTE_KEYS,
  THEME_KEY,
  isPalette,
  readPalette,
  readThemeMode,
  resolveTheme,
} from './theme';

describe('theme', () => {
  it('defaults to following the system', () => {
    expect(readThemeMode(undefined)).toBe('system');
    expect(readThemeMode({ getItem: () => null })).toBe('system');
    expect(readThemeMode({ getItem: () => 'sepia' })).toBe('system');
  });

  it('reads an explicit choice', () => {
    expect(readThemeMode({ getItem: (key) => (key === THEME_KEY ? 'light' : null) })).toBe('light');
    expect(readThemeMode({ getItem: () => 'dark' })).toBe('dark');
  });

  it('survives storage that throws', () => {
    expect(
      readThemeMode({
        getItem: () => {
          throw new Error('denied');
        },
      }),
    ).toBe('system');
  });

  it('resolves System from the operating system, and an explicit choice over it', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });

  it('opens a palette named in the address in its own scheme', () => {
    const saved = { getItem: () => 'light' };
    expect(readThemeMode(saved, '?palette=contrast-dark')).toBe('dark');
    expect(readThemeMode(saved, '?palette=contrast-light')).toBe('light');
    expect(readThemeMode(saved, '?palette=nonsense')).toBe('light');
  });
});

describe('palettes', () => {
  it('has distinct ids, some of each scheme, and none named like the layout default', () => {
    const ids = PALETTES.map((palette) => palette.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).not.toContain(LAYOUT_PALETTE);
    expect(PALETTES.some((palette) => palette.scheme === 'dark')).toBe(true);
    expect(PALETTES.some((palette) => palette.scheme === 'light')).toBe(true);
    for (const palette of PALETTES)
      for (const colour of Object.values(palette.swatch)) expect(colour).toMatch(/^#[0-9a-f]{6}$/);
  });

  it('has Google colours in both schemes', () => {
    for (const scheme of ['dark', 'light'] as const)
      for (const colour of Object.values(LAYOUT_SWATCHES['focus'][scheme]))
        expect(colour).toMatch(/^#[0-9a-f]{6}$/);
  });

  it('knows a palette only in its own scheme', () => {
    expect(isPalette('contrast-dark', 'dark')).toBe(true);
    expect(isPalette('contrast-dark', 'light')).toBe(false);
    expect(isPalette(LAYOUT_PALETTE, 'dark')).toBe(false);
    expect(isPalette(null, 'light')).toBe(false);
  });

  it('reads one palette per scheme, and the layout colours otherwise', () => {
    const storage = {
      getItem: (key: string) =>
        key === PALETTE_KEYS.dark ? 'contrast-dark' : key === PALETTE_KEYS.light ? 'contrast-dark' : null,
    };
    expect(readPalette(storage, 'dark')).toBe('contrast-dark');
    // A dark palette saved as the light one is not applied to light.
    expect(readPalette(storage, 'light')).toBe(LAYOUT_PALETTE);
    expect(readPalette(undefined, 'dark')).toBe(LAYOUT_PALETTE);
    expect(
      readPalette(
        {
          getItem: () => {
            throw new Error('denied');
          },
        },
        'light',
      ),
    ).toBe(LAYOUT_PALETTE);
  });

  it('lets the address pick a palette over the saved one', () => {
    const storage = { getItem: () => LAYOUT_PALETTE };
    expect(readPalette(storage, 'dark', '?demo&palette=contrast-dark')).toBe('contrast-dark');
    expect(readPalette(storage, 'dark', '?palette=contrast-light')).toBe(LAYOUT_PALETTE);
  });
});
