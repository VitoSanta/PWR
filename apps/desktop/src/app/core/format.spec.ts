import { bytes, fitTone, fullness, parameters, percent, shortDate, tokens } from './format';

describe('format', () => {
  it('writes sizes as the OS reports them', () => {
    expect(bytes(16 * 1024 ** 3)).toBe('16 GB');
    expect(bytes(2.26 * 1024 ** 3)).toBe('2.3 GB');
    expect(bytes(512 * 1024 ** 2)).toBe('512 MB');
    expect(bytes(null)).toBe('unknown');
  });

  it('writes tokens compactly and never invents a count', () => {
    expect(tokens(54_000)).toBe('54k');
    expect(tokens(262_144)).toBe('262k');
    expect(tokens(1_500_000)).toBe('1.5M');
    expect(tokens(900)).toBe('900');
    expect(tokens(undefined)).toBe('—');
  });

  it('clamps percentages and survives an empty window', () => {
    expect(percent(54, 128)).toBe(42);
    expect(percent(200, 100)).toBe(100);
    expect(percent(10, 0)).toBe(0);
  });

  it('names parameters and leaves unknown ones unknown', () => {
    expect(parameters(4.02e9)).toBe('4.0B');
    expect(parameters(27.3e9)).toBe('27B');
    expect(parameters(135e6)).toBe('135M');
    expect(parameters(null)).toBe('unknown');
  });

  it('colours fit levels and fullness against the compaction threshold', () => {
    expect(fitTone('recommended')).toBe('ok');
    expect(fitTone('tight_fit')).toBe('warn');
    expect(fitTone('incompatible')).toBe('bad');
    expect(fitTone('unknown')).toBe('muted');
    expect(fullness(80, 75)).toBe('high');
    expect(fullness(60, 75)).toBe('mid');
    expect(fullness(20, 75)).toBe('low');
  });
});

describe('shortDate', () => {
  const now = new Date(2026, 8, 24, 15, 0);
  it('shows the time today, then Yesterday, a weekday, a date', () => {
    expect(shortDate(new Date(2026, 8, 24, 9, 5).toISOString(), now)).toMatch(/9|09/);
    expect(shortDate(new Date(2026, 8, 23, 22, 0).toISOString(), now)).toBe('Yesterday');
    expect(shortDate(new Date(2026, 8, 20).toISOString(), now)).toBe('Sun');
    expect(shortDate(new Date(2026, 5, 2).toISOString(), now)).toBe('2 Jun');
    expect(shortDate(new Date(2025, 1, 3).toISOString(), now)).toMatch(/2025/);
  });

  it('is empty for nothing or an invalid date', () => {
    expect(shortDate(null, now)).toBe('');
    expect(shortDate('not a date', now)).toBe('');
  });
});
