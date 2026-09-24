import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { ToastService } from '../core/ui';
import { Icon } from './kit/icon';

const STEPS = [
  'Preparing Python',
  'Installing the MLX engine',
  'Downloading the search encoder',
  'Checking the installation',
];

/**
 * First run: PWR cannot run a model until its MLX engine exists. This
 * explains what is installed and where, installs it with progress, and hands
 * over to the app -- the next thing a new person needs is a model.
 */
@Component({
  selector: 'pa-engine-setup',
  imports: [Icon],
  template: `
    <div class="setup">
      <div class="setup-titlebar" data-tauri-drag-region="deep"></div>
      <main class="setup-body" aria-labelledby="setup-title">
        <img class="setup-mark" src="/pwr-mark-96.png" alt="" width="48" height="48" />
        @if (store.engineInstalled()) {
          <h1 class="t-display" id="setup-title">PWR is ready</h1>
          <p class="setup-lead">
            The engine is installed. Next, choose a model that fits this Mac: the Model Manager rates each one
            for this machine's memory.
          </p>
        } @else {
          <h1 class="t-display" id="setup-title">Welcome to PWR</h1>
          <p class="setup-lead">
            PWR runs models on this Mac with Apple's MLX. First it installs its engine: a private Python
            environment, used only by PWR. It takes a few minutes, once.
          </p>
        }

        @if (engine(); as engine) {
          @if (!engine.supported) {
            <p class="banner banner-danger" role="alert">
              <pa-icon name="alert" [size]="16" />
              The MLX engine needs a Mac with Apple silicon (M1 or later). This Mac cannot run it.
            </p>
          } @else {
            <section class="setup-card card" [attr.aria-busy]="store.engineInstalling()">
              @if (store.engineInstalling() || store.engineError() || store.engineInstalled()) {
                <ol class="setup-steps">
                  @for (label of steps; track label; let index = $index) {
                    <li class="setup-step" [class]="'setup-step is-' + stepState(index + 1)">
                      <span class="setup-step-icon" aria-hidden="true">
                        @switch (stepState(index + 1)) {
                          @case ('done') {
                            <pa-icon name="check" [size]="14" [stroke]="2.25" />
                          }
                          @case ('active') {
                            <span class="spinner spinner-sm"></span>
                          }
                          @case ('failed') {
                            <pa-icon name="x" [size]="14" [stroke]="2.25" />
                          }
                          @default {
                            <span class="setup-step-dot"></span>
                          }
                        }
                      </span>
                      <span class="setup-step-label">{{ label }}</span>
                      <span class="sr-only">{{ stepState(index + 1) }}</span>
                    </li>
                  }
                </ol>
                <div class="progress" role="progressbar" aria-label="Engine installation" aria-valuemin="0" aria-valuemax="100" [attr.aria-valuenow]="percent()">
                  <span [style.width.%]="percent()"></span>
                </div>
                @if (store.engineInstalling() && lastLine()) {
                  <p class="setup-line mono truncate" aria-live="off">{{ lastLine() }}</p>
                }
              } @else {
                <ul class="setup-list">
                  <li>
                    <pa-icon name="terminal" [size]="16" />
                    <span><strong>Python {{ engine.pythonVersion }}</strong><small>standalone — the system's Python is not touched</small></span>
                  </li>
                  <li>
                    <pa-icon name="cpu" [size]="16" />
                    <span><strong>MLX engine</strong><small class="mono">{{ packageNames() }}</small></span>
                  </li>
                  <li>
                    <pa-icon name="search" [size]="16" />
                    <span><strong>Search encoder</strong><small>multilingual-e5-small, for finding code and docs</small></span>
                  </li>
                </ul>
              }
              <footer class="setup-foot">
                <span class="path-token" [attr.title]="engine.location">
                  <pa-icon name="folder" [size]="14" />
                  <span class="truncate">{{ shortLocation() }}</span>
                </span>
                <span class="t-meta">About 1.2 GB · downloaded once</span>
              </footer>
            </section>

            @if (store.engineError(); as error) {
              <div class="banner banner-danger setup-error" role="alert">
                <pa-icon name="alert" [size]="16" />
                <div class="flex-1">
                  <p>{{ firstLine(error) }}</p>
                  @if (error.includes('\\n') || store.engineLog().length) {
                    <details class="setup-details">
                      <summary>Details</summary>
                      <pre class="selectable">{{ details(error) }}</pre>
                    </details>
                  }
                </div>
              </div>
            }

            <div class="setup-actions">
              @if (store.engineInstalled()) {
                <button class="btn btn-primary btn-lg" (click)="store.finishEngineSetup()" data-autofocus>
                  Continue <pa-icon name="arrow-right" [size]="16" />
                </button>
              } @else if (store.engineInstalling()) {
                <button class="btn btn-lg" (click)="store.cancelEngineInstall()">Cancel</button>
              } @else if (store.engineError()) {
                <button class="btn btn-lg btn-ghost" (click)="copy()">
                  <pa-icon name="copy" [size]="16" /> Copy details
                </button>
                <button class="btn btn-primary btn-lg" (click)="store.installEngine()">
                  <pa-icon name="refresh" [size]="16" /> Try again
                </button>
              } @else {
                <button class="btn btn-primary btn-lg" (click)="store.installEngine()">
                  <pa-icon name="download" [size]="16" /> Install engine
                </button>
              }
            </div>
            @if (!store.engineInstalled()) {
              <p class="fine setup-note">
                Needs an internet connection. To use an environment of your own, start PWR with
                <code class="path-token">PWR_MLX_PYTHON</code> set to its Python.
              </p>
            }
          }
        }
      </main>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EngineSetup {
  protected readonly store = inject(AgentStore);
  private readonly toast = inject(ToastService);
  protected readonly steps = STEPS;
  protected readonly engine = this.store.engine;

  protected readonly packageNames = computed(
    () => this.engine()?.packages.map((name) => name.split('==')[0]).join(' · ') ?? '',
  );

  protected readonly shortLocation = computed(() =>
    (this.engine()?.location ?? '').replace(/^\/Users\/[^/]+/, '~'),
  );

  protected readonly lastLine = computed(() => {
    const log = this.store.engineLog();
    return log[log.length - 1] ?? '';
  });

  /** Steps done, weighted: installing the packages is most of the wait. */
  protected readonly percent = computed(() => {
    if (this.store.engineInstalled()) return 100;
    const step = this.store.engineProgress()?.step ?? 0;
    const weights = [8, 62, 25, 5];
    return weights.slice(0, Math.max(0, step - 1)).reduce((sum, weight) => sum + weight, 0);
  });

  protected stepState(step: number): 'done' | 'active' | 'failed' | 'pending' {
    if (this.store.engineInstalled()) return 'done';
    const current = this.store.engineProgress()?.step ?? 0;
    if (step < current) return 'done';
    if (step === current) {
      if (this.store.engineInstalling()) return 'active';
      if (this.store.engineError()) return 'failed';
    }
    return 'pending';
  }

  protected firstLine(error: string): string {
    return error.split('\n')[0];
  }

  protected details(error: string): string {
    const log = this.store.engineLog().slice(-40).join('\n');
    return [error, log].filter(Boolean).join('\n\n');
  }

  protected async copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(this.details(this.store.engineError()));
      this.toast.show('Copied to clipboard', 'success', 1800);
    } catch {
      this.toast.show('Could not copy to the clipboard', 'danger');
    }
  }
}
