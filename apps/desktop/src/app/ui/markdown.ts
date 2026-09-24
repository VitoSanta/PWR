import { ChangeDetectionStrategy, Component, ElementRef, afterRenderEffect, computed, inject, input } from '@angular/core';
import DOMPurify from 'dompurify';
import { marked } from 'marked';

marked.setOptions({ gfm: true, breaks: true });

const COPY_ICON =
  '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2Z"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>';
const CHECK_ICON =
  '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>';

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
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  protected readonly html = computed(() =>
    DOMPurify.sanitize(marked.parse(this.text(), { async: false }) as string),
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
