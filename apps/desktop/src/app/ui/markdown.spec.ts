import { settle } from './markdown';

describe('settle', () => {
  it('closes a bold or a code span still being written on the last line', () => {
    expect(settle('Project, Architecture, **Abo')).toBe('Project, Architecture, **Abo**');
    expect(settle('run `npm te')).toBe('run `npm te`');
    expect(settle('a `x ** y` and **bo')).toBe('a `x ** y` and **bo**');
  });

  it('holds back a marker with nothing after it yet', () => {
    expect(settle('Next, **')).toBe('Next, ');
    expect(settle('run `')).toBe('run ');
  });

  it('leaves balanced text, earlier lines and open code blocks alone', () => {
    expect(settle('**done** and `code`')).toBe('**done** and `code`');
    expect(settle('**first\nsecond')).toBe('**first\nsecond');
    expect(settle('```ts\nconst a = **b')).toBe('```ts\nconst a = **b');
  });
});
