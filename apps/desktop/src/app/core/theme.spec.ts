import { THEME_KEY, readThemeMode, resolveTheme } from './theme';

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
});
