import { ChangeDetectionStrategy, Component, DestroyRef, OnInit, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { CORE_TOO_OLD } from '../core/personal.store';
import { Icon } from './kit/icon';
import { Tooltip } from './kit/tooltip';
import { Markdown } from './markdown';

/** What `_pwr/wiki` answers. */
interface WikiView {
  overview: string;
  outline: string;
  generatedAt: string;
  nodes: number;
  edges: number;
  modules: { id: string; label: string; summary?: string; stale?: boolean; model?: string }[];
  work: { when: string; request: string; files: string[] }[];
  answer: string | null;
}

/**
 * Inspector → Wiki: what PWR knows about this workspace. The graph and the
 * overview are computed from the files; module summaries are written by the
 * model in the background and marked as such.
 */
@Component({
  selector: 'pa-wiki',
  imports: [Icon, Tooltip, Markdown],
  template: `
    <div class="wiki">
      @if (error()) {
        <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ error() }}</p>
      }
      @if (view(); as wiki) {
        <section class="wiki-section">
          <header class="output-head">
            <span class="truncate">Knowledge graph · {{ wiki.nodes }} nodes · {{ wiki.edges }} links</span>
            <span class="spacer"></span>
            <button class="icon-btn icon-btn-sm" (click)="load()" [disabled]="loading()" aria-label="Refresh" paTooltip="Refresh">
              <pa-icon name="refresh" [size]="14" />
            </button>
          </header>
          <p class="fine">{{ wiki.outline }}</p>
          <form class="wiki-search" (submit)="$event.preventDefault(); ask(question.value)">
            <input
              #question
              class="input input-sm"
              placeholder="A file, folder, symbol or package"
              aria-label="Ask the graph"
            />
            <button class="btn btn-sm" type="submit" [disabled]="loading()">Ask</button>
          </form>
          @if (wiki.answer) {
            <pre class="output wiki-answer">{{ wiki.answer }}</pre>
          }
        </section>

        <section class="wiki-section">
          <h3 class="section-label">Modules</h3>
          @for (module of wiki.modules; track module.id) {
            <article class="wiki-module">
              <button class="wiki-module-name mono" (click)="ask(module.label)">{{ module.label }}</button>
              @if (module.summary) {
                <p>{{ module.summary }}</p>
                <span class="badge" [class.badge-warning]="module.stale">
                  {{ module.stale ? 'written before its files changed' : 'written by ' + (module.model || 'the model') + ', unverified' }}
                </span>
              } @else {
                <p class="t-meta">No summary yet: written in the background when PWR is idle.</p>
              }
            </article>
          } @empty {
            <p class="fine">No source folders found.</p>
          }
        </section>

        <section class="wiki-section">
          <h3 class="section-label">Work done here</h3>
          @for (entry of wiki.work; track $index) {
            <article class="wiki-work">
              <span class="t-meta">{{ entry.when }}</span>
              <span>{{ entry.request }}</span>
              @if (entry.files.length) {
                <span class="t-meta mono">{{ entry.files.join(', ') }}</span>
              }
            </article>
          } @empty {
            <p class="fine">Nothing recorded yet. Each turn that changes or finishes something is logged.</p>
          }
        </section>

        <details class="wiki-section">
          <summary class="section-label">Overview (.pwr/wiki/overview.md)</summary>
          <pa-markdown [text]="wiki.overview" [copyable]="false" />
        </details>
      } @else if (loading()) {
        <p class="fine loading-line"><span class="spinner spinner-sm" aria-hidden="true"></span> Building the wiki…</p>
      } @else if (!agent.workspace() || agent.chatMode()) {
        <p class="fine">Open a workspace to see its wiki.</p>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Wiki implements OnInit {
  protected readonly agent = inject(AgentStore);
  protected readonly view = signal<WikiView | null>(null);
  protected readonly loading = signal(false);
  protected readonly error = signal('');
  private query = '';

  constructor() {
    const stop = this.agent.on('_pwr/wiki_updated', (params) => {
      if (params.cwd === this.agent.workspace()) void this.load();
    });
    inject(DestroyRef).onDestroy(stop);
  }

  ngOnInit(): void {
    void this.load();
  }

  protected ask(question: string): void {
    this.query = question.trim();
    void this.load();
  }

  protected async load(): Promise<void> {
    if (!this.agent.workspace() || this.agent.chatMode()) return;
    this.loading.set(true);
    this.error.set('');
    try {
      this.view.set(
        (await this.agent.call('_pwr/wiki', { cwd: this.agent.workspace(), query: this.query })) as WikiView,
      );
    } catch (error) {
      const text = String(error).replace(/^Error: /, '');
      this.error.set(/method not found/i.test(text) ? CORE_TOO_OLD : text);
    } finally {
      this.loading.set(false);
    }
  }
}
