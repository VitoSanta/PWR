import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
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
import { Tooltip } from './kit/tooltip';

/**
 * The model in use, in the top bar. Opens to switch between the models the
 * engine has, adjust the working window, and reach the Model Manager.
 */
@Component({
  selector: 'pa-model-picker',
  imports: [Icon, Popover, Tooltip],
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
        [anchorAlign]="align()"
        [side]="side()"
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

          <section class="popover-section model-section" aria-labelledby="model-list-label">
            @if (store.models().length > 5) {
              <input
                class="input input-sm model-filter"
                type="search"
                placeholder="Filter…"
                aria-label="Filter models"
                [value]="query()"
                (input)="query.set($any($event.target).value)"
                spellcheck="false"
                autocomplete="off"
              />
            }
            <h3 class="section-label" id="model-list-label">On this machine</h3>
            <div class="model-list" role="radiogroup" aria-labelledby="model-list-label" (keydown)="listKeys($event)">
              @for (ref of shown(); track ref) {
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
                  <span class="model-text">
                    <span class="model-name truncate">{{ label(ref) }}</span>
                    <span class="model-meta truncate">{{ org(ref) }}</span>
                  </span>
                  @if (store.visionModels().includes(ref)) {
                    <span class="model-extra">sees images</span>
                  }
                </button>
              } @empty {
                <p class="fine">{{ store.models().length ? 'No model matches.' : 'No models on this machine yet. Find one in the Model Manager.' }}</p>
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
        </div>
        <footer class="popover-foot model-foot" aria-label="Generation settings">
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
            <span class="effort-select">
              <select
                aria-labelledby="effort-label"
                [value]="store.reasoningEffort()"
                (change)="setEffort($any($event.target).value)"
                [disabled]="store.turnActive() || !store.reasoning()?.applies"
              >
                @for (option of effortOptions; track option.value) {
                  <option [value]="option.value" [selected]="option.value === store.reasoningEffort()">{{ option.label }}</option>
                }
              </select>
              <pa-icon name="chevron-down" [size]="16" aria-hidden="true" />
            </span>
          </div>
          <p class="fine">{{ reasoningHelp(store.reasoning()) }}</p>
          <div class="foot-line">
            <span class="fine">{{ store.models().length }} local model{{ store.models().length === 1 ? '' : 's' }}</span>
            <button class="link-btn" (click)="manage()">Manage →</button>
          </div>
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
  protected readonly store = inject(AgentStore);
  private readonly models = inject(ModelsStore);
  protected readonly open = signal(false);
  protected readonly query = signal('');
  protected readonly label = modelLabel;

  /** The models on this machine, as the filter leaves them. */
  protected readonly shown = computed(() => {
    const words = this.query().toLowerCase().split(/\s+/).filter(Boolean);
    return this.store.models().filter((ref) => words.every((word) => ref.toLowerCase().includes(word)));
  });

  /** Who published it: "lmstudio-community" of "lmstudio-community/Qwen3…". */
  protected org(ref: string): string {
    const slash = ref.indexOf('/');
    return slash > 0 ? ref.slice(0, slash) : 'local';
  }
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

  protected readonly effortOptions = (['low', 'medium', 'high'] as const).map((value) => ({
    value,
    label: value.charAt(0).toUpperCase() + value.slice(1),
  }));

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
