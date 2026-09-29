import { ChangeDetectionStrategy, Component, ElementRef, afterRenderEffect, computed, inject, input } from '@angular/core';
import DOMPurify from 'dompurify';
import { marked } from 'marked';

marked.setOptions({ gfm: true, breaks: true });

const COPY_ICON =
  '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2Z"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>';
const CHECK_ICON =
  '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>';

/**
 * Text still arriving, made safe to render: a bold or a code span opened on
 * the last line and not yet closed is closed for now, so it shows as bold
 * or code as it grows instead of as raw asterisks and backticks. A marker
 * with nothing after it yet is held back. Inside an open code block,
 * nothing is touched.
 */
export function settle(text: string): string {
  if ((text.match(/^\s*(```|~~~)/gm)?.length ?? 0) % 2) return text;
  const start = text.lastIndexOf('\n') + 1;
  let line = text.slice(start);
  let tail = '';
  // Code spans first: asterisks inside one are not markers.
  const ticks = line.match(/`/g)?.length ?? 0;
  if (ticks % 2) {
    if (line.endsWith('`')) line = line.slice(0, -1);
    else tail = '`';
  }
  const outside = (tail ? line.slice(0, line.lastIndexOf('`')) : line).replace(/`[^`]*`/g, '');
  if ((outside.match(/\*\*/g)?.length ?? 0) % 2) {
    if (line.endsWith('**') && !tail) line = line.slice(0, -2);
    else tail = tail ? tail + '**' : '**';
  }
  return text.slice(0, start) + line + tail;
}

/**
 * The end of text that is still streaming in, for a window that shows only
 * its end: `size` characters, from the start of a line, so the window stays
 * full and still and the work per streamed piece stays the same however long
 * the text grows. A cut inside a code block reopens it, or the rest would
 * read inside out.
 *
 * Live reasoning showed its last three paragraphs until 2026-09-29: a new
 * paragraph dropped the oldest, the window shrank and grew again and moved
 * everything below it, and a model that writes no blank lines -- Gemma 4 --
 * had its whole reasoning parsed and redrawn for every few characters.
 */
export function streamTail(text: string, size: number): string {
  const whole = text.trimEnd();
  if (whole.length <= size) return whole;
  let start = whole.length - size;
  const line = whole.indexOf('\n', start);
  if (line >= 0 && line < whole.length - size / 4) start = line + 1;
  const fences = whole.slice(0, start).match(/^\s*(```|~~~)/gm)?.length ?? 0;
  return (fences % 2 ? '```\n' : '') + whole.slice(start);
}

/** Markdown, rendered as it streams in, sanitised before it reaches the DOM. */
@Component({
  selector: 'pa-markdown',
  template: `<div class="markdown" [innerHTML]="html()"></div>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Markdown {
  readonly text = input.required<string>();
  /** A copy button on each code block; off for reasoning, which is read, not reused. */
  readonly copyable = input(true);
  /** Still arriving: an unfinished bold or code span at the end renders as one. */
  readonly streaming = input(false);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  protected readonly html = computed(() =>
    DOMPurify.sanitize(
      marked.parse(this.streaming() ? settle(this.text()) : this.text(), { async: false }) as string,
    ),
  );

  constructor() {
    // Added after rendering: Angular's sanitiser would strip a button from the HTML.
    afterRenderEffect(() => {
      this.html();
      if (this.copyable()) this.addCopyButtons();
    });
  }

  private addCopyButtons(): void {
    for (const pre of Array.from(this.host.nativeElement.querySelectorAll('pre'))) {
      if (pre.querySelector(':scope > .code-copy')) continue;
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'icon-btn icon-btn-sm code-copy';
      button.setAttribute('aria-label', 'Copy code');
      button.innerHTML = COPY_ICON;
      button.addEventListener('click', async () => {
        const code = pre.querySelector('code')?.textContent ?? pre.textContent ?? '';
        try {
          await navigator.clipboard.writeText(code);
          button.innerHTML = CHECK_ICON;
          button.setAttribute('aria-label', 'Copied');
          setTimeout(() => {
            button.innerHTML = COPY_ICON;
            button.setAttribute('aria-label', 'Copy code');
          }, 1400);
        } catch {
          /* clipboard unavailable */
        }
      });
      pre.append(button);
    }
  }
}
