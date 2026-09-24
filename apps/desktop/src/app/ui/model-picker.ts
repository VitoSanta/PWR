import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  inject,
  signal,
} from '@angular/core';
import { AgentStore } from '../core/agent.store';
import {
  EFFORT_HELP,
  capabilityResult,
  confidenceLabel,
  offersFirstChoice,
  reasoningHelp,
  statusTitle,
  testedCapabilities,
} from '../core/compatibility';
import { tokens } from '../core/format';
import { modelLabel } from '../core/model';
import { ModelsStore } from '../core/models.store';

/**
 * The model in use, in the top bar. Opens to switch between the models the
 * engine has, adjust the working window, and reach the Model Manager.
 */
@Component({
  selector: 'pa-model-picker',
  template: `
    <button
      class="model-chip"
      (click)="open.set(!open())"
      [attr.aria-expanded]="open()"
      aria-haspopup="menu"
      [class.missing]="!store.model()"
    >
      <span class="chip-dot" [class.ok]="!!store.model() && !store.backendNote()"></span>
      {{ store.modelName() }}
      <span class="chev-down">▾</span>
    </button>

    @if (open()) {
      <div class="popover model-pop" role="menu">
        <header>
          <h3>Model</h3>
          <button class="icon-button" (click)="open.set(false)" aria-label="Close">×</button>
        </header>
        @if (store.backendNote()) {
          <p class="warn">{{ store.backendNote() }}</p>
        }
        <div class="model-list">
          @for (ref of store.models(); track ref) {
            <button
              class="model-option"
              role="menuitemradio"
              [attr.aria-checked]="ref === store.model()"
              [class.current]="ref === store.model()"
              (click)="choose(ref)"
              [disabled]="store.turnActive()"
              [title]="ref"
            >
              <span class="radio"></span>
              <span class="model-name">{{ label(ref) }}</span>
              @if (store.visionModels().includes(ref)) {
                <span class="tag">sees images</span>
              }
            </button>
          } @empty {
            <p class="muted">No models on this machine yet. Find one in the Model Manager.</p>
          }
        </div>

        @if (store.compatibility(); as compatibility) {
          <section class="window compat" aria-label="Model compatibility">
            <strong>{{ title(compatibility) }}</strong>
            @switch (compatibility.status) {
              @case ('provisional') {
                @if (firstChoice(compatibility)) {
                  <p class="fine">This model has not yet been tested by PWR.</p>
                } @else if (compatibility.reasons?.length) {
                  @for (reason of compatibility.reasons; track reason) {
                    <p class="fine">{{ sentence(reason) }}</p>
                  }
                } @else {
                  <p class="fine">
                    Untested: PWR uses conservative defaults. Nothing about this model has been
                    measured yet.
                  </p>
                }
              }
              @case ('limited') {
                <p class="fine">{{ compatibility.features?.note }}</p>
                <p class="fine">Available: chat without a workspace.</p>
                <p class="fine">
                  Suggested: run Quick Calibration again, try another quantization, or use another
                  model.
                </p>
              }
              @case ('incompatible') {
                @for (reason of compatibility.reasons ?? []; track reason) {
                  <p class="fine">{{ reason }}</p>
                }
              }
              @default {
                <p class="fine">Profile confidence: {{ confidence(compatibility) }}</p>
                @for (reason of compatibility.reasons ?? []; track reason) {
                  <p class="fine">{{ sentence(reason) }}</p>
                }
              }
            }
            @if (tested(compatibility).length) {
              <dl class="capabilities">
                @for (line of tested(compatibility); track line.label) {
                  <dt>{{ line.label }}</dt>
                  <dd [class.bad]="line.result === 'not_reliable'">{{ result(line.result) }}</dd>
                }
              </dl>
            }
            @if (store.calibrating()) {
              <p class="fine" aria-live="polite">
                @if (store.calibrationProgress(); as progress) {
                  Checking {{ progress.name }} ({{ progress.step }} of {{ progress.total }})…
                } @else {
                  Starting bounded local checks…
                }
              </p>
              <button class="ghost" (click)="store.cancelQuickCalibration()">Cancel</button>
            } @else if (compatibility.status !== 'incompatible') {
              <div class="row actions">
                @if (compatibility.recalibrate || compatibility.status === 'limited') {
                  <button
                    class="ghost"
                    (click)="store.quickCalibrate()"
                    [disabled]="store.turnActive()"
                    title="About nine short requests, checked mechanically. Takes a minute or two on most models."
                  >
                    Run Quick Calibration
                  </button>
                }
                @if (firstChoice(compatibility)) {
                  <button class="ghost" (click)="store.useConservativeDefaults()">
                    Use Conservative Defaults
                  </button>
                }
              </div>
            }
            @if (store.calibrationError()) {
              <p class="warn">{{ store.calibrationError() }}</p>
            }
          </section>
        }

        @if (store.context(); as context) {
          <section class="window">
            <div class="row">
              <span>Working window</span>
              <div class="stepper">
                <button
                  (click)="store.stepContext(-1)"
                  [disabled]="store.turnActive()"
                  aria-label="Smaller window"
                >
                  −
                </button>
                <strong>{{ t(context.tokens) }}</strong>
                <button
                  (click)="store.stepContext(1)"
                  [disabled]="store.turnActive()"
                  aria-label="Larger window"
                >
                  ＋
                </button>
              </div>
            </div>
            <p class="fine">
              {{
                context.setting
                  ? 'Your setting, capped by what this machine holds.'
                  : 'Computed for this machine.'
              }}
              @if (context.rationale) {
                {{ sentence(context.rationale) }}
              }
            </p>
          </section>
        }

        <section class="window" aria-label="Reasoning Effort">
          <div class="row">
            <span
              title="How much of a reply the model may spend thinking before it must answer or act. Not the number of steps or tool calls."
              >Reasoning Effort</span
            >
            <select
              [value]="store.reasoningEffort()"
              [disabled]="store.turnActive() || !store.reasoning()?.applies"
              (change)="setEffort($event)"
              aria-label="Reasoning Effort"
            >
              <option value="low" [title]="help.low">Low</option>
              <option value="medium" [title]="help.medium">Medium</option>
              <option value="high" [title]="help.high">High</option>
            </select>
          </div>
          <p class="fine">{{ reasoningHelp(store.reasoning()) }}</p>
        </section>

        <footer>
          <button class="ghost wide" (click)="manage()">Manage models…</button>
        </footer>
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ModelPicker {
  protected readonly store = inject(AgentStore);
  private readonly models = inject(ModelsStore);
  private readonly host = inject(ElementRef<HTMLElement>);
  protected readonly open = signal(false);
  protected readonly label = modelLabel;
  protected readonly t = tokens;

  protected sentence(text: string): string {
    const trimmed = text.trim().replace(/\.$/, '');
    return trimmed.charAt(0).toUpperCase() + trimmed.slice(1) + '.';
  }

  protected readonly title = statusTitle;
  protected readonly confidence = confidenceLabel;
  protected readonly firstChoice = offersFirstChoice;
  protected readonly tested = testedCapabilities;
  protected readonly result = capabilityResult;
  protected readonly reasoningHelp = reasoningHelp;
  protected readonly help = EFFORT_HELP;

  protected choose(ref: string): void {
    if (ref !== this.store.model()) void this.store.selectModel(ref);
  }

  protected setEffort(event: Event): void {
    const effort = (event.target as HTMLSelectElement).value;
    if (effort === 'low' || effort === 'medium' || effort === 'high')
      void this.store.setReasoningEffort(effort);
  }

  protected manage(): void {
    this.open.set(false);
    this.models.show();
  }

  @HostListener('document:mousedown', ['$event'])
  protected outside(event: MouseEvent): void {
    if (this.open() && !this.host.nativeElement.contains(event.target as Node))
      this.open.set(false);
  }

  @HostListener('document:keydown.escape')
  protected escape(): void {
    this.open.set(false);
  }
}
