import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { FileDiff } from '../core/model';

interface Line {
  kind: 'same' | 'add' | 'del' | 'gap';
  text: string;
}

/**
 * A compact unified view of one edit: the unchanged head and tail are
 * trimmed to a little context, and the changed middle is shown as removed
 * then added lines.
 */
export function diffLines(oldText: string, newText: string, context = 3): Line[] {
  const before = oldText.split('\n');
  const after = newText.split('\n');
  let head = 0;
  while (head < before.length && head < after.length && before[head] === after[head]) head++;
  let tail = 0;
  while (
    tail < before.length - head &&
    tail < after.length - head &&
    before[before.length - 1 - tail] === after[after.length - 1 - tail]
  )
    tail++;
  const lines: Line[] = [];
  if (head > context) lines.push({ kind: 'gap', text: `${head - context} unchanged lines` });
  for (const text of before.slice(Math.max(0, head - context), head)) lines.push({ kind: 'same', text });
  for (const text of before.slice(head, before.length - tail)) lines.push({ kind: 'del', text });
  for (const text of after.slice(head, after.length - tail)) lines.push({ kind: 'add', text });
  const trailing = after.slice(after.length - tail);
  for (const text of trailing.slice(0, context)) lines.push({ kind: 'same', text });
  if (tail > context) lines.push({ kind: 'gap', text: `${tail - context} unchanged lines` });
  return lines;
}

/** Lines added and removed by one edit, for a compact summary. */
export function diffStats(oldText: string, newText: string): { added: number; removed: number } {
  const lines = diffLines(oldText, newText, 0);
  return {
    added: lines.filter((line) => line.kind === 'add').length,
    removed: lines.filter((line) => line.kind === 'del').length,
  };
}

@Component({
  selector: 'pa-diff',
  template: `
    <div class="diff">
      @for (line of lines(); track $index) {
        <div class="diff-line" [class]="line.kind">
          <span class="sign">{{ line.kind === 'add' ? '+' : line.kind === 'del' ? '−' : line.kind === 'gap' ? '⋯' : ' ' }}</span>
          <span class="code">{{ line.text }}</span>
        </div>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Diff {
  readonly diff = input.required<FileDiff>();
  protected readonly lines = computed(() => {
    const all = diffLines(this.diff().oldText, this.diff().newText);
    return all.length > 400 ? [...all.slice(0, 400), { kind: 'gap' as const, text: `${all.length - 400} more lines` }] : all;
  });
}
