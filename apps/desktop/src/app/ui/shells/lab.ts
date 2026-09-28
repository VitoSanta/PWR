import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { LayoutService } from '../../core/layout';
import { WindowState } from '../../core/navigation';
import { RUN_STATE_LABEL, RunStore } from '../../core/run';
import { UiStore } from '../../core/ui';
import { VARIANTS, Variant, VariantService } from '../../core/variant';
import { Composer } from '../composer';
import { ContextMeter } from '../context-meter';
import { Conversation } from '../conversation';
import { Inspector } from '../inspector';
import { Icon } from '../kit/icon';
import { ModelPicker } from '../model-picker';
import { DiagnosticExport } from '../parts/diagnostic-export';
import { PhaseStrip } from '../parts/phase-strip';
import { RunBoard } from '../parts/run-board';
import { SessionSwitcher } from '../parts/session-switcher';
import { StatusBar } from '../parts/status-bar';
import { ToolDock } from '../parts/tool-dock';
import { MemoryProposals } from '../personal';
import { RunMetricsChip } from '../run-metrics';

type DeckPage = 'task' | 'dialogue' | 'tools';

/** Six ways of composing the real PWR surfaces, without a second copy of app state. */
@Component({
  selector: 'pa-shell-lab',
  imports: [Composer, ContextMeter, Conversation, DiagnosticExport, Icon, Inspector,
    MemoryProposals, ModelPicker, PhaseStrip, RunBoard, RunMetricsChip,
    SessionSwitcher, StatusBar, ToolDock],
  template: `
    <div [class]="'lab-shell lab-' + variants.variant()"
      [class.is-mac]="win.isMac" [class.is-fullscreen]="win.fullscreen()"
      [class.is-working]="run.state() === 'working'"
      [class.is-verifying]="run.current() === 'Verifying' || run.current() === 'Fixing'">
      <header class="lab-head titlebar-row" data-tauri-drag-region="deep">
        <pa-session-switcher />
        <span class="lab-head-divider" aria-hidden="true"></span>
        <label class="lab-layout-picker">
          <span class="sr-only">Layout</span>
          <select [value]="variants.variant()" (change)="choose($event)" aria-label="Compare layouts">
            @for (item of layouts; track item.id) {
              <option [value]="item.id" [selected]="item.id === variants.variant()">{{ item.label }}</option>
            }
          </select>
        </label>
        <span class="lab-philosophy truncate">{{ variants.info().description }}</span>
        <span class="spacer" data-tauri-drag-region="deep"></span>
        <span [class]="'run-state is-' + run.state()" role="status">{{ state() }}</span>
        <pa-context-meter />
        <pa-run-metrics />
        <pa-model-picker />
        <pa-diagnostic-export />
        <button class="icon-btn" (click)="ui.settingsOpen.set(true)" aria-label="Appearance and settings">
          <pa-icon name="settings" />
        </button>
      </header>

      @if (variants.variant() === 'deck') {
        <nav class="lab-deck-nav" aria-label="Deck surfaces">
          <button [attr.aria-current]="deck() === 'task' ? 'page' : null" (click)="deck.set('task')">Task <span>{{ run.actions().length }} actions</span></button>
          <button [attr.aria-current]="deck() === 'dialogue' ? 'page' : null" (click)="deck.set('dialogue')">Dialogue</button>
          <button [attr.aria-current]="deck() === 'tools' ? 'page' : null" (click)="deck.set('tools')">Tools</button>
        </nav>
      }

      <div class="lab-body">
        @if (variants.variant() === 'relay' || variants.variant() === 'chronicle' || variants.variant() === 'pulse' || variants.variant() === 'workshop') {
          <aside class="lab-phases" aria-label="Agent phases">
            <div class="lab-region-head"><span class="section-label">Run sequence</span><span class="lab-live-label">{{ run.current() || 'Ready' }}</span></div>
            <pa-phase-strip [vertical]="variants.variant() === 'relay'" />
          </aside>
        }

        @if (variants.variant() === 'map' || variants.variant() === 'pulse' || variants.variant() === 'workshop' || (variants.variant() === 'deck' && deck() === 'task')) {
          <section class="lab-task" aria-label="Task activity">
            <pa-run-board />
          </section>
        }

        @if (variants.variant() !== 'deck' || deck() === 'dialogue') {
          <main class="lab-chat main" aria-label="Conversation">
            @if (store.coreError()) {
              <div class="core-error banner banner-danger" role="alert">
                <pa-icon name="alert" [size]="16" />
                <span class="selectable">{{ store.coreError() }}</span>
              </div>
            }
            @if (variants.variant() === 'chronicle' || variants.variant() === 'map') {
              <div class="lab-chat-intro">
                <span class="section-label">{{ variants.variant() === 'chronicle' ? 'Operational record' : 'Dialogue' }}</span>
                <span class="lab-live-label">{{ run.actions().length }} actions in this run</span>
              </div>
            }
            <pa-conversation />
            <pa-memory-proposals />
            <pa-composer />
          </main>
        }

        @if (variants.variant() !== 'deck' || deck() === 'tools') {
          <section class="lab-tools" aria-label="Tools and evidence">
            <div class="lab-region-head">
              <span class="section-label">Workbench</span>
              <button class="btn btn-sm btn-ghost" (click)="layout.toggleRight()"
                [attr.aria-expanded]="layout.right() !== 'hidden'">
                {{ layout.right() === 'hidden' ? 'Show tools' : 'Hide tools' }}
              </button>
            </div>
            @if (layout.right() !== 'hidden') { <pa-inspector /> }
            @else { <pa-tool-dock /> }
          </section>
        }
      </div>

      <pa-status-bar />
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LabShell {
  protected readonly store = inject(AgentStore);
  protected readonly run = inject(RunStore);
  protected readonly variants = inject(VariantService);
  protected readonly layout = inject(LayoutService);
  protected readonly win = inject(WindowState);
  protected readonly ui = inject(UiStore);
  protected readonly layouts = VARIANTS;
  protected readonly deck = signal<DeckPage>('task');
  protected readonly state = computed(() => RUN_STATE_LABEL[this.run.state()]);

  constructor() {
    this.layout.leftFixed.set(0);
  }

  protected choose(event: Event): void {
    this.variants.set((event.target as HTMLSelectElement).value as Variant);
  }
}
