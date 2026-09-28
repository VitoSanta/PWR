import { LAYOUT_SWATCHES, PALETTES } from './palettes';
import { VARIANTS } from './variant';
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
    expect(readThemeMode(saved, '?palette=midnight')).toBe('dark');
    expect(readThemeMode(saved, '?palette=solar')).toBe('light');
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

  it('has the own colours of every layout, in both schemes', () => {
    for (const variant of VARIANTS)
      for (const scheme of ['dark', 'light'] as const)
        for (const colour of Object.values(LAYOUT_SWATCHES[variant.id][scheme]))
          expect(colour).toMatch(/^#[0-9a-f]{6}$/);
  });

  it('knows a palette only in its own scheme', () => {
    expect(isPalette('midnight', 'dark')).toBe(true);
    expect(isPalette('midnight', 'light')).toBe(false);
    expect(isPalette(LAYOUT_PALETTE, 'dark')).toBe(false);
    expect(isPalette(null, 'light')).toBe(false);
  });

  it('reads one palette per scheme, and the layout colours otherwise', () => {
    const storage = {
      getItem: (key: string) =>
        key === PALETTE_KEYS.dark ? 'ember' : key === PALETTE_KEYS.light ? 'midnight' : null,
    };
    expect(readPalette(storage, 'dark')).toBe('ember');
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
    const storage = { getItem: () => 'ember' };
    expect(readPalette(storage, 'dark', '?demo&palette=arctic')).toBe('arctic');
    expect(readPalette(storage, 'dark', '?palette=solar')).toBe('ember');
  });
});
