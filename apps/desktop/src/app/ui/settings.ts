import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { EngineStatus } from '../core/model';
import { ThemeMode, ThemeService } from '../core/theme';
import { SHORTCUTS, UiStore, roveFocus, shortcut } from '../core/ui';
import { Dialog } from './kit/dialog';
import { Icon, IconName } from './kit/icon';
import { MemorySettings, ProfileSettings, ProjectsSettings } from './personal';

type Page = 'profile' | 'memory' | 'projects' | 'appearance' | 'shortcuts';

/** Preferences, one page at a time: kept out of the main screen. */
@Component({
  selector: 'pa-settings',
  imports: [Dialog, Icon, ProfileSettings, MemorySettings, ProjectsSettings],
  template: `
    @if (ui.settingsOpen()) {
      <pa-dialog
        size="lg"
        labelledBy="settings-title"
        (closed)="ui.settingsOpen.set(false)"
        animate.leave="is-leaving"
      >
        <div class="settings-layout">
          <nav class="settings-nav" aria-labelledby="settings-title" (keydown)="navKeys($event)">
            <h2 class="dialog-title settings-nav-title" id="settings-title">Settings</h2>
            @for (item of pages; track item.id) {
              <button
                class="settings-nav-item"
                [attr.aria-current]="page() === item.id ? 'page' : null"
                (click)="page.set(item.id)"
                [attr.data-autofocus]="page() === item.id ? '' : null"
              >
                <pa-icon [name]="item.icon" [size]="16" /> {{ item.label }}
              </button>
            }
          </nav>
          <div class="settings-content">
            <button class="icon-btn settings-close" (click)="ui.settingsOpen.set(false)" aria-label="Close settings">
              <pa-icon name="x" />
            </button>
            @switch (page()) {
              @case ('profile') {
                <pa-profile-settings />
              }
              @case ('memory') {
                <pa-memory-settings />
              }
              @case ('projects') {
                <pa-projects-settings />
              }
              @case ('appearance') {
                <header class="settings-page-head">
                  <h3 class="settings-page-title">Appearance</h3>
                  <p class="fine">System follows your operating system's appearance and changes with it.</p>
                </header>
                <div
                  class="theme-options"
                  role="radiogroup"
                  aria-label="Appearance"
                  (keydown)="themeKeys($event)"
                >
                  @for (option of themes; track option.value) {
                    <button
                      class="theme-option"
                      role="radio"
                      [attr.aria-checked]="theme.mode() === option.value"
                      [attr.tabindex]="theme.mode() === option.value ? 0 : -1"
                      (click)="theme.set(option.value)"
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
                @if (store.engine(); as engine) {
                  @if (engine.needed) {
                    <div class="settings-row">
                      <div class="settings-row-text">
                        <span class="settings-row-title">MLX engine</span>
                        @if (engine.python) {
                          <span class="t-meta mono truncate">{{ engine.python }}</span>
                        }
                      </div>
                      <span class="badge" [class.badge-success]="engine.ready" [class.badge-warning]="!engine.ready">
                        {{ engine.ready ? sourceLabel(engine.source) : 'not installed' }}
                      </span>
                    </div>
                  }
                }
              }
              @case ('shortcuts') {
                <header class="settings-page-head">
                  <h3 class="settings-page-title">Keyboard shortcuts</h3>
                </header>
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
              }
            }
          </div>
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
  protected readonly page = signal<Page>('profile');
  protected readonly pages: { id: Page; label: string; icon: IconName }[] = [
    { id: 'profile', label: 'Profile', icon: 'heart' },
    { id: 'memory', label: 'Memory', icon: 'history' },
    { id: 'projects', label: 'Projects', icon: 'folder' },
    { id: 'appearance', label: 'Appearance', icon: 'sun' },
    { id: 'shortcuts', label: 'Shortcuts', icon: 'command' },
  ];
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

  protected navKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '.settings-nav-item', 'vertical'))
      (document.activeElement as HTMLElement | null)?.click();
  }

  protected themeKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]', 'horizontal'))
      (document.activeElement as HTMLElement | null)?.click();
  }
}
