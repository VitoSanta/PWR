import { settle, streamTail } from './markdown';

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

describe('streamTail', () => {
  it('keeps short text whole', () => {
    expect(streamTail('one\ntwo\n', 900)).toBe('one\ntwo');
  });

  it('gives a fixed amount from the start of a line, however long the text', () => {
    const line = 'The surname gives three consonants, then vowels, then X.\n';
    const text = line.repeat(400);
    const tail = streamTail(text, 300);
    expect(tail.length).toBeLessThanOrEqual(300);
    expect(tail.length).toBeGreaterThan(200);
    expect(tail.startsWith('The surname')).toBe(true);
    // Growing the text does not grow the tail.
    expect(streamTail(text + line.repeat(400), 300).length).toBe(tail.length);
  });

  it('reopens a code block the cut falls inside', () => {
    const text = 'Plan:\n```csharp\n' + 'var x = 1;\n'.repeat(60) + 'var last = 2;';
    const tail = streamTail(text, 120);
    expect(tail.startsWith('```\n')).toBe(true);
    expect(tail.endsWith('var last = 2;')).toBe(true);
  });

  it('does not open one the cut falls after', () => {
    const text = '```\ncode\n```\n' + 'Then the check character.\n'.repeat(40);
    expect(streamTail(text, 200).startsWith('```')).toBe(false);
  });
});
