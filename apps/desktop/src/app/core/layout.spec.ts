import { LEFT, MAIN_MIN, RAIL, RIGHT, arrange } from './layout';

const open = {
  leftOpen: true,
  rightOpen: true,
  leftWidth: LEFT.initial,
  rightWidth: RIGHT.initial,
  leftPeek: false,
  rightPeek: false,
};

describe('arrange', () => {
  it('docks both panels in a large window', () => {
    for (const width of [1600, 1440, 1280]) {
      const result = arrange(width, open);
      expect(result.left).toBe('docked');
      expect(result.right).toBe('docked');
      expect(width - LEFT.initial - RIGHT.initial).toBeGreaterThanOrEqual(MAIN_MIN);
    }
  });

  it('collapses the inspector first as the window narrows', () => {
    for (const width of [1100, 900]) {
      const result = arrange(width, open);
      expect(result.left).toBe('docked');
      expect(result.right).toBe('hidden');
    }
  });

  it('collapses the navigation to the rail when even it does not fit', () => {
    const result = arrange(720, open);
    expect(result.left).toBe('rail');
    expect(result.right).toBe('hidden');
  });

  it('never leaves the conversation narrower than its minimum beside docked panels', () => {
    for (let width = 640; width <= 2000; width += 10) {
      const result = arrange(width, open);
      const left = result.left === 'docked' ? LEFT.initial : RAIL;
      const right = result.right === 'docked' ? RIGHT.initial : 0;
      if (result.left === 'docked' || result.right === 'docked') {
        expect(width - left - right).toBeGreaterThanOrEqual(MAIN_MIN);
      }
    }
  });

  it('floats a panel over the conversation when asked for where it cannot dock', () => {
    expect(arrange(1100, { ...open, rightPeek: true }).right).toBe('overlay');
    expect(arrange(720, { ...open, leftPeek: true }).left).toBe('overlay');
  });

  it('keeps a panel the person closed closed, even with room for it', () => {
    const result = arrange(1600, { ...open, leftOpen: false, rightOpen: false });
    expect(result.left).toBe('rail');
    expect(result.right).toBe('hidden');
    // The inspector gets the room the collapsed navigation gave back.
    expect(arrange(1100, { ...open, leftOpen: false }).right).toBe('docked');
  });
});
