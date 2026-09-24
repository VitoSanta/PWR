import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import DOMPurify from 'dompurify';
import { marked } from 'marked';

marked.setOptions({ gfm: true, breaks: true });

/** Markdown, rendered as it streams in, sanitised before it reaches the DOM. */
@Component({
  selector: 'pa-markdown',
  template: `<div class="markdown" [innerHTML]="html()"></div>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Markdown {
  readonly text = input.required<string>();
  protected readonly html = computed(() =>
    DOMPurify.sanitize(marked.parse(this.text(), { async: false }) as string),
  );
}
