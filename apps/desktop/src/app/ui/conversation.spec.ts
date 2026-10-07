import { describe, expect, it } from 'vitest';
import { clockTime } from './conversation';

describe('clockTime', () => {
  it('reads an instant as the local time of day, on 24 hours and to the second', () => {
    expect(clockTime(new Date(2026, 9, 7, 14, 7, 5).getTime())).toBe('14:07:05');
    expect(clockTime(new Date(2026, 9, 7, 0, 0, 0).getTime())).toBe('00:00:00');
    expect(clockTime(new Date(2026, 9, 7, 23, 59, 59).getTime())).toBe('23:59:59');
  });
});
