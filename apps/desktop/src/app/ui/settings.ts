import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { EngineStatus } from '../core/model';
import { LAYOUT_PALETTE, Theme, ThemeMode, ThemeService } from '../core/theme';
import { LAYOUT_SWATCHES, PALETTES, Swatch } from '../core/palettes';
import { VARIANTS, VariantService } from '../core/variant';
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
            <button
              class="icon-btn settings-close"
              (click)="ui.settingsOpen.set(false)"
              aria-label="Close settings"
            >
              <pa-icon name="x" />
            </button>
            @switch (page()) {
              @case ('profile') {
                <pa-profile-settings animate.enter="anim-fade-in" />
              }
              @case ('memory') {
                <pa-memory-settings animate.enter="anim-fade-in" />
              }
              @case ('projects') {
                <pa-projects-settings animate.enter="anim-fade-in" />
              }
              @case ('appearance') {
                <div class="settings-page" animate.enter="anim-fade-in">
                  <header class="settings-page-head">
                    <h3 class="settings-page-title">Appearance</h3>
                    <p class="fine">
                      System follows your operating system's appearance and changes with it.
                    </p>
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
                          [style]="modeSwatch(option.value)"
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
                  <header class="settings-page-head settings-subhead">
                    <h4 class="settings-row-title">Colour theme</h4>
                    <p class="fine">
                      One for dark and one for light; System switches between them. Any layout, any
                      colours.
                    </p>
                  </header>
                  @for (scheme of schemes; track scheme.id) {
                    <div class="colour-row">
                      <span class="colour-row-title"
                        ><pa-icon [name]="scheme.icon" [size]="14" /> {{ scheme.label }}</span
                      >
                      <div
                        class="colour-options"
                        role="radiogroup"
                        [attr.aria-label]="'Colour theme ' + scheme.label"
                        (keydown)="themeKeys($event)"
                      >
                        @for (option of palettesFor(scheme.id); track option.id) {
                          <button
                            class="colour-option"
                            role="radio"
                            [attr.aria-checked]="chosenPalette(scheme.id) === option.id"
                            [attr.tabindex]="chosenPalette(scheme.id) === option.id ? 0 : -1"
                            [attr.title]="option.description"
                            (click)="theme.setPalette(scheme.id, option.id)"
                          >
                            <span
                              class="colour-swatch"
                              [style]="paletteSwatch(scheme.id, option.id)"
                              aria-hidden="true"
                            >
                              <span class="colour-swatch-side"></span>
                              <span class="colour-swatch-main"><i></i><i></i><b></b></span>
                            </span>
                            <span class="colour-label">{{ option.label }}</span>
                          </button>
                        }
                      </div>
                    </div>
                  }
                  <header class="settings-page-head settings-subhead">
                    <h4 class="settings-row-title">Layout</h4>
                    <p class="fine">
                      Every layout has every feature; they arrange them differently.
                    </p>
                  </header>
                  <div
                    class="variant-options"
                    role="radiogroup"
                    aria-label="Layout"
                    (keydown)="variantKeys($event)"
                  >
                    @for (option of variantList; track option.id) {
                      <button
                        class="variant-option"
                        role="radio"
                        [attr.aria-checked]="variants.variant() === option.id"
                        [attr.tabindex]="variants.variant() === option.id ? 0 : -1"
                        (click)="variants.set(option.id)"
                      >
                        <span [class]="'variant-sketch sketch-' + option.id" aria-hidden="true"
                          ><i></i><i></i><i></i
                        ></span>
                        <span class="variant-text">
                          <span class="variant-label">{{ option.label }}</span>
                          <span class="fine">{{ option.description }}</span>
                        </span>
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
                        <span
                          class="badge"
                          [class.badge-success]="engine.ready"
                          [class.badge-warning]="!engine.ready"
                        >
                          {{ engine.ready ? sourceLabel(engine.source) : 'not installed' }}
                        </span>
                      </div>
                    }
                  }
                </div>
              }
              @case ('shortcuts') {
                <div class="settings-page" animate.enter="anim-fade-in">
                  <header class="settings-page-head">
                    <h3 class="settings-page-title">Shortcuts</h3>
                    <p class="fine">Keys that work anywhere in PWR.</p>
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
                </div>
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
  protected readonly variants = inject(VariantService);
  protected readonly variantList = VARIANTS;
  protected readonly themes: { value: ThemeMode; label: string; icon: IconName }[] = [
    { value: 'system', label: 'System', icon: 'monitor' },
    { value: 'light', label: 'Light', icon: 'sun' },
    { value: 'dark', label: 'Dark', icon: 'moon' },
  ];
  protected readonly shortcuts = [
    { label: 'Command palette', keys: SHORTCUTS.palette },
    { label: 'New conversation', keys: SHORTCUTS.newConversation },
    { label: 'Show or hide the sidebar', keys: SHORTCUTS.toggleSidebar },
    { label: 'Show or hide the workbench', keys: SHORTCUTS.toggleInspector },
    { label: 'Settings', keys: SHORTCUTS.settings },
    { label: 'Send message', keys: 'Enter' },
    { label: 'New line', keys: 'Shift+Enter' },
  ];

  protected readonly schemes: { id: Theme; label: string; icon: IconName }[] = [
    { id: 'dark', label: 'When dark', icon: 'moon' },
    { id: 'light', label: 'When light', icon: 'sun' },
  ];

  /** The layout's own colours first, then every palette of the scheme. */
  protected palettesFor(scheme: Theme): { id: string; label: string; description: string }[] {
    return [
      {
        id: LAYOUT_PALETTE,
        label: 'Layout colours',
        description: "The chosen layout's own colours",
      },
      ...PALETTES.filter((palette) => palette.scheme === scheme),
    ];
  }

  protected chosenPalette(scheme: Theme): string {
    return scheme === 'dark' ? this.theme.darkPalette() : this.theme.lightPalette();
  }

  /** What a scheme looks like in one of its palettes, or in the current layout's own colours. */
  private swatchFor(scheme: Theme, id: string): Swatch {
    const palette = PALETTES.find((item) => item.id === id && item.scheme === scheme);
    return (
      palette?.swatch ??
      (LAYOUT_SWATCHES[this.variants.variant()] ?? LAYOUT_SWATCHES['studio'])[scheme]
    );
  }

  protected paletteSwatch(scheme: Theme, id: string): Record<string, string> {
    const swatch = this.swatchFor(scheme, id);
    return {
      '--sw-side': swatch.sidebar,
      '--sw-main': swatch.surface,
      '--sw-accent': swatch.accent,
      '--sw-text': swatch.text,
      '--sw-second': swatch.secondary,
    };
  }

  /** System, Light and Dark, each previewed in the palette it would use. */
  protected modeSwatch(mode: ThemeMode): Record<string, string> {
    const style: Record<string, string> = {};
    for (const scheme of ['light', 'dark'] as const) {
      if (mode !== 'system' && mode !== scheme) continue;
      const swatch = this.swatchFor(scheme, this.chosenPalette(scheme));
      style[`--${scheme}-side`] = swatch.sidebar;
      style[`--${scheme}-app`] = swatch.app;
      style[`--${scheme}-main`] = swatch.surface;
    }
    return style;
  }

  protected sourceLabel(source: EngineStatus['source']): string {
    return {
      installed: 'installed by PWR',
      environment: 'from PWR_MLX_PYTHON',
      checkout: 'from the checkout',
    }[source ?? 'installed'];
  }

  protected navKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '.settings-nav-item', 'vertical'))
      (document.activeElement as HTMLElement | null)?.click();
  }

  protected variantKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]', 'horizontal'))
      (document.activeElement as HTMLElement | null)?.click();
  }

  protected themeKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=radio]', 'horizontal'))
      (document.activeElement as HTMLElement | null)?.click();
  }
}
