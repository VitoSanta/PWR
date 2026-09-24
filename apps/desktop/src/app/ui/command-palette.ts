import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { LayoutService } from '../core/layout';
import { modelLabel } from '../core/model';
import { ModelsStore } from '../core/models.store';
import { ThemeService } from '../core/theme';
import { SHORTCUTS, UiStore, shortcut } from '../core/ui';
import { Dialog } from './kit/dialog';
import { Icon, IconName } from './kit/icon';

interface Command {
  id: string;
  label: string;
  group: string;
  icon: IconName;
  keys?: string;
  hint?: string;
  run: () => void;
}

/**
 * ⌘K / Ctrl+K: every action the app offers, found by typing. Only real
 * actions of the app are listed; each is enabled only when it would work.
 */
@Component({
  selector: 'pa-command-palette',
  imports: [Dialog, Icon],
  template: `
    @if (ui.paletteOpen()) {
      <pa-dialog
        size="md"
        ariaLabel="Command palette"
        (closed)="close()"
        animate.leave="is-leaving"
        class="palette-dialog"
      >
        <div class="palette-search">
          <pa-icon name="search" [size]="16" />
          <input
            class="palette-input"
            type="text"
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-autocomplete="list"
            [attr.aria-activedescendant]="results().length ? 'palette-' + active() : null"
            placeholder="Type a command…"
            [value]="query()"
            (input)="search($any($event.target).value)"
            (keydown)="keys($event)"
            data-autofocus
            spellcheck="false"
            autocomplete="off"
          />
          <span class="kbd" aria-hidden="true">esc</span>
        </div>
        <div class="palette-list" id="palette-list" role="listbox" aria-label="Commands">
          @for (command of results(); track command.id; let index = $index) {
            @if (index === 0 || results()[index - 1].group !== command.group) {
              <div class="menu-label" role="presentation">{{ command.group }}</div>
            }
            <div
              class="menu-item palette-item"
              role="option"
              [id]="'palette-' + index"
              [class.is-active]="index === active()"
              [attr.aria-selected]="index === active()"
              (pointermove)="active.set(index)"
              (pointerdown)="$event.preventDefault()"
              (click)="run(command)"
            >
              <pa-icon [name]="command.icon" [size]="16" />
              <span class="truncate">{{ command.label }}</span>
              @if (command.hint) {
                <span class="menu-hint">{{ command.hint }}</span>
              }
              @if (command.keys) {
                <span class="kbd palette-keys">{{ shortcut(command.keys) }}</span>
              }
            </div>
          } @empty {
            <div class="empty-state">
              <p class="empty-state-title">No matching commands</p>
            </div>
          }
        </div>
      </pa-dialog>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CommandPalette {
  protected readonly ui = inject(UiStore);
  private readonly store = inject(AgentStore);
  private readonly layout = inject(LayoutService);
  private readonly models = inject(ModelsStore);
  private readonly theme = inject(ThemeService);
  protected readonly shortcut = shortcut;
  protected readonly query = signal('');
  protected readonly active = signal(0);

  private readonly commands = computed<Command[]>(() => {
    const store = this.store;
    const idle = !store.turnActive();
    const list: Command[] = [];
    if (idle) {
      list.push({
        id: 'new',
        label: 'New conversation',
        group: 'Conversation',
        icon: 'square-pen',
        keys: SHORTCUTS.newConversation,
        run: () => store.newConversation(),
      });
    }
    if (store.turnActive()) {
      list.push({
        id: 'stop',
        label: 'Stop this turn',
        group: 'Conversation',
        icon: 'stop',
        run: () => store.cancel(),
      });
    }
    if (idle && store.sessionId()) {
      list.push({
        id: 'compact',
        label: 'Compact context now',
        group: 'Conversation',
        icon: 'refresh',
        run: () => void store.compactNow(),
      });
    }
    if (idle && !store.switchingMode() && !store.switchingWorkspace()) {
      if (store.chatMode())
        list.push({
          id: 'agent',
          label: 'Switch to Agent mode',
          group: 'Workspace',
          icon: 'terminal',
          run: () => void store.leaveChat(),
        });
      else if (store.chatHome())
        list.push({
          id: 'chat',
          label: 'Switch to Chat mode',
          group: 'Workspace',
          icon: 'message',
          run: () => void store.openChat(),
        });
      list.push({
        id: 'workspace',
        label: 'Open a workspace folder…',
        group: 'Workspace',
        icon: 'folder',
        run: () => void store.chooseWorkspace(),
      });
    }
    if (idle) {
      list.push({
        id: 'attach-files',
        label: 'Attach files…',
        group: 'Workspace',
        icon: 'file',
        run: () => void store.attachFiles(),
      });
      list.push({
        id: 'attach-folder',
        label: 'Attach a folder…',
        group: 'Workspace',
        icon: 'folder',
        hint: 'read-only',
        run: () => void store.attachFolder(),
      });
    }
    for (const [name, label, icon] of [
      ['verify', 'Verify', 'shield-check'],
      ['changes', 'List changes', 'git-compare'],
      ['report', 'Report', 'file-text'],
      ['diagnose', 'Diagnose', 'activity'],
      ['doctor', 'Doctor', 'stethoscope'],
    ] as const) {
      if (!store.sessionId() || store.commandRunning()) break;
      list.push({
        id: 'cmd-' + name,
        label,
        group: 'Evidence',
        icon,
        run: () => this.evidence(name),
      });
    }
    list.push({
      id: 'models',
      label: 'Open Model Manager',
      group: 'Models',
      icon: 'box',
      run: () => this.models.show(),
    });
    if (idle) {
      for (const ref of store.models()) {
        if (ref === store.model()) continue;
        list.push({
          id: 'model-' + ref,
          label: `Use ${modelLabel(ref)}`,
          group: 'Models',
          icon: 'box',
          run: () => void store.selectModel(ref),
        });
      }
    }
    list.push(
      {
        id: 'sidebar',
        label: 'Toggle sidebar',
        group: 'View',
        icon: 'panel-left',
        keys: SHORTCUTS.toggleSidebar,
        run: () => this.layout.toggleLeft(),
      },
      {
        id: 'inspector',
        label: 'Toggle inspector',
        group: 'View',
        icon: 'panel-right',
        keys: SHORTCUTS.toggleInspector,
        run: () => this.layout.toggleRight(),
      },
      {
        id: 'theme-system',
        label: 'Appearance: System',
        group: 'View',
        icon: 'monitor',
        hint: this.theme.mode() === 'system' ? 'current' : undefined,
        run: () => this.theme.set('system'),
      },
      {
        id: 'theme-light',
        label: 'Appearance: Light',
        group: 'View',
        icon: 'sun',
        hint: this.theme.mode() === 'light' ? 'current' : undefined,
        run: () => this.theme.set('light'),
      },
      {
        id: 'theme-dark',
        label: 'Appearance: Dark',
        group: 'View',
        icon: 'moon',
        hint: this.theme.mode() === 'dark' ? 'current' : undefined,
        run: () => this.theme.set('dark'),
      },
      {
        id: 'settings',
        label: 'Settings',
        group: 'View',
        icon: 'settings',
        keys: SHORTCUTS.settings,
        run: () => this.ui.settingsOpen.set(true),
      },
    );
    return list;
  });

  protected readonly results = computed(() => {
    const words = this.query().toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return this.commands();
    return this.commands().filter((command) => {
      const text = `${command.group} ${command.label}`.toLowerCase();
      return words.every((word) => text.includes(word));
    });
  });

  protected search(value: string): void {
    this.query.set(value);
    this.active.set(0);
  }

  protected keys(event: KeyboardEvent): void {
    const count = this.results().length;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!count) return;
      this.active.update((index) => (index + (event.key === 'ArrowDown' ? 1 : -1) + count) % count);
      document.getElementById('palette-' + this.active())?.scrollIntoView?.({ block: 'nearest' });
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const command = this.results()[this.active()];
      if (command) this.run(command);
    }
  }

  protected run(command: Command): void {
    this.close();
    // After the palette has gone, so a command that opens a dialog gets focus.
    setTimeout(() => command.run());
  }

  protected close(): void {
    this.ui.paletteOpen.set(false);
    this.query.set('');
    this.active.set(0);
  }

  private evidence(name: 'verify' | 'changes' | 'report' | 'diagnose' | 'doctor'): void {
    void this.store.runCommand(name);
    // Show the output where it appears.
    this.ui.inspectorTab.set('evidence');
    if (this.layout.right() === 'hidden') this.layout.toggleRight();
  }
}
