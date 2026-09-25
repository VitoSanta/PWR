import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { EngineStatus } from '../core/model';
import { ThemeMode, ThemeService } from '../core/theme';
import { SHORTCUTS, UiStore, roveFocus, shortcut } from '../core/ui';
import { Dialog } from './kit/dialog';
import { Icon, IconName } from './kit/icon';
import { PersonalSettings } from './personal';

/** Preferences: kept out of the main screen. */
@Component({
  selector: 'pa-settings',
  imports: [Dialog, Icon, PersonalSettings],
  template: `
    @if (ui.settingsOpen()) {
      <pa-dialog
        size="md"
        labelledBy="settings-title"
        (closed)="ui.settingsOpen.set(false)"
        animate.leave="is-leaving"
      >
        <header class="dialog-header">
          <div class="dialog-header-text">
            <h2 class="dialog-title" id="settings-title">Settings</h2>
            <p class="dialog-description">Stored on this machine.</p>
          </div>
          <button class="icon-btn" (click)="ui.settingsOpen.set(false)" aria-label="Close settings">
            <pa-icon name="x" />
          </button>
        </header>
        <div class="dialog-body settings-body">
          <pa-personal-settings />

          <section class="settings-group" aria-labelledby="appearance-label">
            <h3 class="section-label" id="appearance-label">Appearance</h3>
            <div
              class="theme-options"
              role="radiogroup"
              aria-labelledby="appearance-label"
              (keydown)="themeKeys($event)"
            >
              @for (option of themes; track option.value) {
                <button
                  class="theme-option"
                  role="radio"
                  [attr.aria-checked]="theme.mode() === option.value"
                  [attr.tabindex]="theme.mode() === option.value ? 0 : -1"
                  (click)="theme.set(option.value)"
                  [attr.data-autofocus]="theme.mode() === option.value ? '' : null"
                >
                  <span
                    class="theme-swatch"
                    [class]="'theme-swatch swatch-' + option.value"
                    aria-hidden="true"
                  >
                    <span></span><span></span>
                  </span>
                  <span class="theme-label"
                    ><pa-icon [name]="option.icon" [size]="14" /> {{ option.label }}</span
                  >
                </button>
              }
            </div>
            <p class="fine">
              System follows your operating system’s appearance and changes with it.
            </p>
          </section>

          @if (store.engine(); as engine) {
            @if (engine.needed) {
              <section class="settings-group" aria-labelledby="engine-label">
                <h3 class="section-label" id="engine-label">Engine</h3>
                <dl class="shortcut-list">
                  <div>
                    <dt>MLX environment</dt>
                    <dd>
                      <span class="badge" [class.badge-success]="engine.ready" [class.badge-warning]="!engine.ready">
                        {{ engine.ready ? sourceLabel(engine.source) : 'not installed' }}
                      </span>
                    </dd>
                  </div>
                </dl>
                @if (engine.python) {
                  <p class="dialog-subject">{{ engine.python }}</p>
                }
              </section>
            }
          }

          <section class="settings-group" aria-labelledby="shortcuts-label">
            <h3 class="section-label" id="shortcuts-label">Keyboard shortcuts</h3>
            <dl class="shortcut-list">
              @for (item of shortcuts; track item.label) {
                <div>
                  <dt>{{ item.label }}</dt>
                  <dd>
                    <span class="kbd">{{ shortcut(item.keys) }}</span>
                  </dd>
                </div>
              }
            </dl>
          </section>
        </div>
        <div class="dialog-footer">
          <button class="btn" (click)="ui.settingsOpen.set(false)">Done</button>
        </div>
      </pa-dialog>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Settings {
  protected readonly ui = inject(UiStore);
  protected readonly theme = inject(ThemeService);
  protected readonly store = inject(AgentStore);
  protected readonly shortcut = shortcut;
  protected readonly themes: { value: ThemeMode; label: string; icon: IconName }[] = [
    { value: 'system', label: 'System', icon: 'monitor' },
    { value: 'light', label: 'Light', icon: 'sun' },
    { value: 'dark', label: 'Dark', icon: 'moon' },
  ];
  protected readonly shortcuts = [
    { label: 'Command palette', keys: SHORTCUTS.palette },
    { label: 'New conversation', keys: SHORTCUTS.newConversation },
    { label: 'Show or hide the sidebar', keys: SHORTCUTS.toggleSidebar },
    { label: 'Show or hide the inspector', keys: SHORTCUTS.toggleInspector },
    { label: 'Settings', keys: SHORTCUTS.settings },
    { label: 'Send message', keys: 'Enter' },
    { label: 'New line', keys: 'Shift+Enter' },
  ];

  protected sourceLabel(source: EngineStatus['source']): string {
    return { installed: 'installed by PWR', environment: 'from PWR_MLX_PYTHON', checkout: 'from the checkout' }[
      source ?? 'installed'
    ];
  }

  protected themeKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]', 'horizontal'))
      (document.activeElement as HTMLElement | null)?.click();
  }
}
