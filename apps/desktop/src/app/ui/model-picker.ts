import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import {
  capabilityResult,
  confidenceLabel,
  offersFirstChoice,
  reasoningHelp,
  statusTitle,
  testedCapabilities,
} from '../core/compatibility';
import { tokens } from '../core/format';
import { ModelCompatibility, ReasoningEffort, modelLabel } from '../core/model';
import { ModelsStore } from '../core/models.store';
import { roveFocus } from '../core/ui';
import { Icon } from './kit/icon';
import { Popover } from './kit/popover';
import { Select, SelectOption } from './kit/select';
import { Tooltip } from './kit/tooltip';

/**
 * The model in use, in the top bar. Opens to switch between the models the
 * engine has, adjust the working window, and reach the Model Manager.
 */
@Component({
  selector: 'pa-model-picker',
  imports: [Icon, Popover, Select, Tooltip],
  template: `
    <button
      #chip
      class="topbar-chip"
      (click)="open.set(!open())"
      [attr.aria-expanded]="open()"
      aria-haspopup="dialog"
      [class.is-missing]="!store.model()"
      [attr.aria-label]="'Model: ' + store.modelName()"
      paTooltip="Model, reasoning and working context"
    >
      <span class="dot" [class]="'dot ' + (store.model() && !store.backendNote() ? 'dot-success' : 'dot-warning')" aria-hidden="true"></span>
      <span class="truncate topbar-chip-label">{{ store.modelName() }}</span>
      <pa-icon name="chevron-down" [size]="14" />
    </button>

    @if (open()) {
      <pa-popover
        [anchor]="chip"
        anchorAlign="end"
        width="380px"
        ariaLabel="Model"
        (closed)="open.set(false)"
        animate.leave="anim-pop-out"
      >
        <div class="popover-head">
          <h2 class="popover-title">Model</h2>
          <button class="icon-btn icon-btn-sm" (click)="open.set(false)" aria-label="Close">
            <pa-icon name="x" [size]="16" />
          </button>
        </div>
        <div class="popover-body">
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
                  <span class="model-name truncate">{{ label(ref) }}</span>
                  @if (store.visionModels().includes(ref)) {
                    <span class="badge badge-info"><pa-icon name="eye" [size]="12" /> sees images</span>
                  }
                </button>
              } @empty {
                <p class="fine">No models on this machine yet. Find one in the Model Manager.</p>
              }
            </div>
          </section>

          @if (store.compatibility(); as compatibility) {
            <section class="popover-section" aria-labelledby="compat-title">
              <div class="compat-head">
                <h3 class="section-label" id="compat-title">Compatibility</h3>
                <span class="badge" [class]="'badge ' + statusTone(compatibility.status)">{{ title(compatibility) }}</span>
              </div>
              @switch (compatibility.status) {
                @case ('provisional') {
                  @if (firstChoice(compatibility)) {
                    <p class="fine">This model has not yet been tested by PWR.</p>
                  } @else if (compatibility.reasons?.length) {
                    @for (reason of compatibility.reasons; track reason) {
                      <p class="fine">{{ sentence(reason) }}</p>
                    }
                  } @else {
                    <p class="fine">Untested: PWR uses conservative defaults. Nothing about this model has been measured yet.</p>
                  }
                }
                @case ('limited') {
                  <p class="fine">{{ compatibility.features?.note }}</p>
                  <p class="fine">Available: chat without a workspace.</p>
                  <p class="fine">Suggested: run Quick Calibration again, try another quantization, or use another model.</p>
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
                    <dd [class.text-danger]="line.result === 'not_reliable'">{{ result(line.result) }}</dd>
                  }
                </dl>
              }
              @if (store.calibrating()) {
                <div class="calibration" aria-live="polite">
                  <span class="spinner spinner-sm" aria-hidden="true"></span>
                  <span class="fine flex-1">
                    @if (store.calibrationProgress(); as progress) {
                      Checking {{ progress.name }} ({{ progress.step }} of {{ progress.total }})…
                    } @else {
                      Starting bounded local checks…
                    }
                  </span>
                  <button class="btn btn-sm" (click)="store.cancelQuickCalibration()">Cancel</button>
                </div>
              } @else if (compatibility.status !== 'incompatible' && (compatibility.recalibrate || compatibility.status === 'limited' || firstChoice(compatibility))) {
                <div class="button-row">
                  @if (compatibility.recalibrate || compatibility.status === 'limited') {
                    <button
                      class="btn btn-sm"
                      (click)="store.quickCalibrate()"
                      [disabled]="store.turnActive()"
                      paTooltip="About nine short requests, checked mechanically. Takes a minute or two on most models."
                    >
                      <pa-icon name="gauge" [size]="14" /> Run Quick Calibration
                    </button>
                  }
                  @if (firstChoice(compatibility)) {
                    <button class="btn btn-sm btn-ghost" (click)="store.useConservativeDefaults()">Use Conservative Defaults</button>
                  }
                </div>
              }
              @if (store.calibrationError()) {
                <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ store.calibrationError() }}</p>
              }
            </section>
          }

          <section class="popover-section" aria-label="Generation settings">
            @if (store.context(); as context) {
              <div class="field-row">
                <span class="field-label" id="window-label">Working context</span>
                <div class="stepper" role="group" aria-labelledby="window-label">
                  <button class="icon-btn icon-btn-sm icon-btn-outline" (click)="store.stepContext(-1)" [disabled]="store.turnActive()" aria-label="Smaller window">
                    <pa-icon name="minus" [size]="14" />
                  </button>
                  <strong class="num" aria-live="polite">{{ t(context.tokens) }}</strong>
                  <button class="icon-btn icon-btn-sm icon-btn-outline" (click)="store.stepContext(1)" [disabled]="store.turnActive()" aria-label="Larger window">
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
                id="effort-label"
                paTooltip="How much of a reply the model may spend thinking before it must answer or act. Not the number of steps or tool calls."
                >Reasoning effort</span
              >
              <pa-select
                class="effort-select"
                size="sm"
                labelledBy="effort-label"
                [options]="effortOptions()"
                [value]="store.reasoningEffort()"
                (valueChange)="setEffort($any($event))"
                [disabled]="store.turnActive() || !store.reasoning()?.applies"
              />
            </div>
            <p class="fine">{{ reasoningHelp(store.reasoning()) }}</p>
          </section>
        </div>
        <footer class="popover-foot">
          <button class="btn btn-block" (click)="manage()">
            <pa-icon name="box" [size]="16" /> Manage models…
          </button>
        </footer>
      </pa-popover>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ModelPicker {
  protected readonly store = inject(AgentStore);
  private readonly models = inject(ModelsStore);
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

  protected choose(ref: string): void {
    if (ref !== this.store.model()) void this.store.selectModel(ref);
  }

  protected readonly effortOptions = computed<SelectOption<ReasoningEffort>[]>(() => {
    const budgets = this.store.reasoning()?.control === 'budget' ? this.store.reasoning()?.budgets : null;
    return (['low', 'medium', 'high'] as const).map((value) => ({
      value,
      label: value.charAt(0).toUpperCase() + value.slice(1),
      hint: budgets ? tokens(budgets[value]) : undefined,
    }));
  });

  protected setEffort(effort: ReasoningEffort): void {
    if (effort !== this.store.reasoningEffort()) void this.store.setReasoningEffort(effort);
  }

  protected statusTone(status: ModelCompatibility['status']): string {
    switch (status) {
      case 'verified':
      case 'locally_calibrated':
        return 'badge-success';
      case 'limited':
        return 'badge-warning';
      case 'incompatible':
        return 'badge-danger';
      default:
        return '';
    }
  }

  protected listKeys(event: KeyboardEvent): void {
    roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]');
  }

  protected manage(): void {
    this.open.set(false);
    this.models.show();
  }
}
