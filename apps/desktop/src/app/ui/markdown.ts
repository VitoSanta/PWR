import { ChangeDetectionStrategy, Component, ElementRef, afterRenderEffect, computed, inject, input } from '@angular/core';
import DOMPurify from 'dompurify';
import { marked } from 'marked';

marked.setOptions({ gfm: true, breaks: true });

// Material Symbols glyphs (the font pa-icon draws from), as plain markup:
// the copy button is added to rendered HTML, outside Angular's templates.
const COPY_ICON = '<span class="material-symbols-rounded md-glyph" aria-hidden="true">content_copy</span>';
const CHECK_ICON = '<span class="material-symbols-rounded md-glyph" aria-hidden="true">check</span>';

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
