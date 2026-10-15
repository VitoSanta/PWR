import { MatSlideToggleModule } from '@angular/material/slide-toggle';
import { MatButtonModule } from '@angular/material/button';
import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { tokens } from '../core/format';
import { modelLabel } from '../core/model';
import { ModelsStore } from '../core/models.store';
import { rate, runMetrics } from '../core/trace';
import { UiStore, roveFocus } from '../core/ui';
import { Icon } from './kit/icon';
import { Popover } from './kit/popover';
import { Tooltip } from './kit/tooltip';

/**
 * The model in use, in the top bar. Opens to switch between the models the
 * engine has, adjust the working window, and reach the Model Manager.
 */
@Component({
  selector: 'pa-model-picker',
  imports: [MatSlideToggleModule, MatButtonModule, Icon, Popover, Tooltip],
  template: `
    <button
      #chip
      class="topbar-chip"
      (click)="open.set(!open())"
      [attr.aria-expanded]="open()"
      aria-haspopup="dialog"
      [class.is-missing]="!store.model()"
      [class.is-compact]="compact()"
      [attr.aria-label]="'Model: ' + store.modelName()"
      [paTooltip]="compact() ? store.modelName() + ' · model, thinking and speed' : 'Model, reasoning and working context'"
    >
      <span class="dot" [class]="'dot ' + (store.model() && !store.backendNote() ? 'dot-success' : 'dot-warning')" aria-hidden="true"></span>
      <span class="truncate topbar-chip-label">{{ compact() ? short() : store.modelName() }}</span>
      <pa-icon name="chevron-down" [size]="14" />
    </button>

    @if (open()) {
      <pa-popover
        [anchor]="chip"
        [anchorAlign]="align()"
        [side]="side()"
        width="380px"
        ariaLabel="Model"
        (closed)="open.set(false)"
        animate.leave="anim-pop-out"
      >
        <div class="popover-head">
          <h2 class="popover-title">Model</h2>
          <button matIconButton class="icon-btn icon-btn-sm" (click)="open.set(false)" aria-label="Close">
            <pa-icon name="x" [size]="16" />
          </button>
        </div>
        <div class="popover-body">
          @if (compact()) {
            <!-- The run's speed lives with the model: its figures one step away. -->
            <div class="popover-section">
              <button type="button" class="engine-row" (click)="showMetrics(chip)">
                <pa-icon name="gauge" [size]="16" />
                <span class="engine-row-label">Speed</span>
                <span class="engine-row-value num">{{ speed() ? speed() + ' tok/s' : '—' }}</span>
                <pa-icon name="chevron-right" [size]="16" />
              </button>
            </div>
          }
          @if (store.backendNote()) {
            <div class="popover-section">
              <p class="banner banner-warning"><pa-icon name="alert" [size]="16" />{{ store.backendNote() }}</p>
            </div>
          }

          <section class="popover-section" aria-labelledby="model-list-label">
            <h3 class="section-label" id="model-list-label">On this machine</h3>
            <div class="model-list" role="radiogroup" aria-labelledby="model-list-label" (keydown)="listKeys($event)">
              @for (ref of store.models(); track ref) {
                <button
                  class="model-option"
                  role="radio"
                  [attr.aria-checked]="ref === store.model()"
                  [attr.tabindex]="ref === store.model() || (!store.model() && $first) ? 0 : -1"
                  (click)="choose(ref)"
                  [disabled]="store.turnActive()"
                  [attr.title]="ref"
                >
                  <span class="radio" aria-hidden="true"></span>
                  <span class="model-identity">
                    <span class="model-name truncate">{{ label(ref) }}</span>
                    <span class="model-reference">{{ ref }}</span>
                  </span>
                  @if (store.visionModels().includes(ref)) {
                    <span class="badge badge-info"><pa-icon name="eye" [size]="12" /> sees images</span>
                  }
                </button>
              } @empty {
                <p class="fine">No models on this machine yet. Find one in the Model Manager.</p>
              }
            </div>
          </section>

          <section class="popover-section" aria-label="Generation settings">
            @if (store.context(); as context) {
              <div class="field-row">
                <span class="field-label" id="window-label">Working context</span>
                <div class="stepper" role="group" aria-labelledby="window-label">
                  <button matIconButton class="icon-btn icon-btn-sm icon-btn-outline" (click)="store.stepContext(-1)" [disabled]="store.turnActive()" aria-label="Smaller window">
                    <pa-icon name="minus" [size]="14" />
                  </button>
                  <strong class="num" aria-live="polite">{{ t(context.tokens) }}</strong>
                  <button matIconButton class="icon-btn icon-btn-sm icon-btn-outline" (click)="store.stepContext(1)" [disabled]="store.turnActive()" aria-label="Larger window">
                    <pa-icon name="plus" [size]="14" />
                  </button>
                </div>
              </div>
              <p class="fine">
                {{ context.setting ? 'Your setting, capped by what this machine holds.' : 'Computed for this machine.' }}
                @if (context.rationale) {
                  {{ sentence(context.rationale) }}
                }
              </p>
            }
            <div class="field-row">
              <span
                class="field-label"
                id="thinking-label"
                paTooltip="Gemma thinks before it answers or acts. Slower, sometimes better on hard steps."
                >Thinking</span
              >
              <mat-slide-toggle
                aria-labelledby="thinking-label"
                [checked]="store.thinking()"
                [disabled]="store.turnActive()"
                (change)="store.setThinking($event.checked)"
              />
            </div>
            <p class="fine">
              @if (store.thinking()) {
                On: each reply may think first, up to {{ t(store.thoughtCap()) }} tokens.
              } @else {
                Off: Gemma answers and acts directly.
              }
            </p>
            <div class="field-row">
              <span
                class="field-label"
                id="assistant-label"
                paTooltip="Google's small assistant model drafts a few tokens ahead and Gemma checks them. Experimental: not yet faster or identical on every pack."
                >Assistant <span class="badge badge-outline">experimental</span></span
              >
              <mat-slide-toggle
                aria-labelledby="assistant-label"
                [checked]="store.assistant()"
                [disabled]="store.turnActive()"
                (change)="store.setAssistant($event.checked)"
              />
            </div>
            <p class="fine">
              @if (!store.assistant()) {
                Off: Gemma writes every token itself.
              } @else if (store.assistantInstalled()) {
                On: the assistant drafts and Gemma checks. Not with an 8-bit cache.
              } @else {
                On, but this model's assistant is not downloaded yet:
                <button class="link-btn" (click)="manage()">get it in Models</button>.
              }
            </p>
          </section>
        </div>
        <footer class="popover-foot">
          <button matButton="outlined" class="btn btn-block" (click)="manage()">
            <pa-icon name="box" [size]="16" /> Manage models…
          </button>
        </footer>
      </pa-popover>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ModelPicker {
  /** Where its panel opens: below the chip in a top bar, above it in a status bar. */
  readonly side = input<'bottom' | 'top'>('bottom');
  readonly align = input<'start' | 'end'>('end');
  /** Its icon alone, the name in the tooltip: the message bar's form. */
  readonly compact = input(false);
  protected readonly store = inject(AgentStore);
  private readonly models = inject(ModelsStore);
  protected readonly open = signal(false);
  private readonly ui = inject(UiStore);
  protected readonly label = modelLabel;
  /** The last generation's speed, as the run metrics read it. */
  protected readonly speed = computed(() => {
    const value = runMetrics(this.store.timeline(), Date.now(), this.store.turnActive()).generationTps;
    return value === null ? '' : rate(value);
  });

  /** "Gemma 26B" for "mlx-community/gemma-4-26b-a4b-it-4bit": the chip's short name. */
  protected readonly short = computed(() => {
    const name = this.store.modelName();
    const size = /gemma[-\s]?4[-\s]+(e\d+b|\d+b)/i.exec(name)?.[1];
    return size ? `Gemma ${size.toUpperCase()}` : name;
  });

  protected showMetrics(chip: HTMLElement): void {
    this.open.set(false);
    this.ui.runMetrics.set(chip);
  }
  protected readonly t = tokens;

  protected sentence(text: string): string {
    const trimmed = text.trim().replace(/\.$/, '');
    return trimmed.charAt(0).toUpperCase() + trimmed.slice(1) + '.';
  }

  protected choose(ref: string): void {
    if (ref !== this.store.model()) void this.store.selectModel(ref);
  }

  protected listKeys(event: KeyboardEvent): void {
    roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]');
  }

  protected manage(): void {
    this.open.set(false);
    this.models.show();
  }
}
