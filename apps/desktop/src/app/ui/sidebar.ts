import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { shortDate } from '../core/format';
import { LEFT, LayoutService } from '../core/layout';
import { ConfirmService, SHORTCUTS, ToastService, UiStore, roveFocus, shortcut } from '../core/ui';
import { Icon } from './kit/icon';
import { ResizeHandle } from './kit/resize-handle';
import { Tooltip } from './kit/tooltip';

/** Shared by the full sidebar and the rail. */
abstract class Navigation {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  protected readonly ui = inject(UiStore);
  protected readonly keys = SHORTCUTS;
  protected readonly shortcut = shortcut;
}

@Component({
  selector: 'pa-sidebar',
  imports: [Icon, Tooltip, ResizeHandle],
  template: `
    <nav class="sidebar" aria-label="Navigation">
      <header class="sidebar-head titlebar-row" data-tauri-drag-region="deep">
        <img class="brand-mark" src="/pwr-mark-96.png" alt="" width="22" height="22" />
        <span class="brand-name" data-tauri-drag-region="deep">PWR</span>
        <span class="spacer" data-tauri-drag-region="deep"></span>
        <button
          class="icon-btn icon-btn-sm"
          (click)="layout.toggleLeft()"
          aria-label="Hide sidebar"
          paTooltip="Hide sidebar"
          [paTooltipKeys]="keys.toggleSidebar"
        >
          <pa-icon name="panel-left" [size]="16" />
        </button>
      </header>

      <div class="sidebar-top">
        <div
          class="segmented segmented-block"
          role="radiogroup"
          aria-label="Conversation mode"
          [attr.aria-busy]="switching()"
          (keydown)="modeKeys($event)"
        >
          <button
            role="radio"
            [attr.aria-checked]="!store.chatMode()"
            [attr.tabindex]="store.chatMode() ? -1 : 0"
            (click)="useAgent()"
            [disabled]="modeLocked()"
            paTooltip="Work in a project folder: read, edit and run"
          >
            <pa-icon name="terminal" [size]="14" /> Agent
          </button>
          <button
            role="radio"
            [attr.aria-checked]="store.chatMode()"
            [attr.tabindex]="store.chatMode() ? 0 : -1"
            (click)="useChat()"
            [disabled]="modeLocked() || (!store.chatMode() && !store.chatHome())"
            paTooltip="Talk to the model without a workspace: it reads only what you attach"
          >
            <pa-icon name="message" [size]="14" /> Chat
          </button>
        </div>
        @if (switching()) {
          <p class="sidebar-status" role="status"><span class="spinner spinner-sm" aria-hidden="true"></span>Opening workspace…</p>
        }
        <button class="btn btn-block new-conversation" (click)="store.newConversation()" [disabled]="store.turnActive()">
          <pa-icon name="square-pen" [size]="16" />
          New conversation
          <span class="kbd" aria-hidden="true">{{ shortcut(keys.newConversation) }}</span>
        </button>
      </div>

      <section class="sidebar-section" aria-labelledby="workspace-label">
        <h2 class="section-label" id="workspace-label">{{ store.chatMode() ? 'Chat' : 'Workspace' }}</h2>
        @if (store.chatMode()) {
          <div class="workspace-row" [paTooltip]="'Saved in ' + store.chatHome()">
            <pa-icon name="lock" [size]="16" />
            <span class="workspace-text">
              <span class="workspace-name">No workspace</span>
              <span class="workspace-path truncate">read-only · saved globally</span>
            </span>
          </div>
        } @else {
          <button
            class="workspace-row"
            (click)="store.chooseWorkspace()"
            [disabled]="switching()"
            [paTooltip]="store.workspace() || 'Choose a folder'"
            aria-label="Change workspace folder"
          >
            <pa-icon name="folder" [size]="16" />
            <span class="workspace-text">
              <span class="workspace-name truncate">{{ folderName() }}</span>
              <span class="workspace-path truncate">{{ folderParent() }}</span>
            </span>
            <pa-icon class="workspace-change" name="chevron-right" [size]="16" />
          </button>
        }
      </section>

      <section class="sidebar-section sessions" aria-labelledby="sessions-label">
        <h2 class="section-label" id="sessions-label">Conversations</h2>
        <ul class="session-list">
          @for (session of store.sessions(); track session.sessionId) {
            <li class="session-row" [class.is-current]="session.sessionId === store.sessionId()">
              <button
                class="session"
                (click)="store.resume(session.sessionId)"
                (keydown.delete)="askDelete(session.sessionId, session.title)"
                (keydown.backspace)="$any($event).metaKey && askDelete(session.sessionId, session.title)"
                [disabled]="store.turnActive()"
                [attr.aria-current]="session.sessionId === store.sessionId() ? 'page' : null"
                [attr.title]="session.title || 'Untitled'"
              >
                <span class="session-title truncate">{{ session.title || 'Untitled' }}</span>
                <span class="session-meta">{{ date(session.updatedAt) }}</span>
              </button>
              <button
                class="icon-btn icon-btn-sm icon-btn-danger session-delete"
                (click)="askDelete(session.sessionId, session.title)"
                [disabled]="store.turnActive()"
                [attr.aria-label]="'Delete conversation ' + (session.title || 'Untitled')"
                paTooltip="Delete conversation"
              >
                <pa-icon name="trash" [size]="14" />
              </button>
            </li>
          } @empty {
            <li class="sidebar-empty">No conversations yet.</li>
          }
        </ul>
      </section>

      <footer class="sidebar-foot">
        <span class="core-status" [paTooltip]="coreDetail()">
          <span class="dot" [class]="'dot ' + coreDot()" aria-hidden="true"></span>
          <span class="truncate">{{ coreLabel() }}</span>
        </span>
        <span class="spacer"></span>
        <button
          class="icon-btn icon-btn-sm"
          (click)="ui.paletteOpen.set(true)"
          aria-label="Commands"
          paTooltip="Commands"
          [paTooltipKeys]="keys.palette"
        >
          <pa-icon name="command" [size]="16" />
        </button>
        <button
          class="icon-btn icon-btn-sm"
          (click)="ui.settingsOpen.set(true)"
          aria-label="Settings"
          paTooltip="Settings"
          [paTooltipKeys]="keys.settings"
        >
          <pa-icon name="settings" [size]="16" />
        </button>
      </footer>
    </nav>
    @if (layout.left() === 'docked') {
      <pa-resize-handle
        edge="right"
        label="Resize sidebar"
        [width]="layout.leftWidth()"
        [min]="bounds.min"
        [max]="bounds.max"
        [initial]="bounds.initial"
        (resize)="layout.setLeftWidth($event)"
      />
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Sidebar extends Navigation {
  private readonly confirm = inject(ConfirmService);
  private readonly toast = inject(ToastService);
  protected readonly bounds = LEFT;
  protected readonly date = shortDate;

  protected readonly switching = computed(() => this.store.switchingMode() || this.store.switchingWorkspace());
  protected readonly modeLocked = computed(() => this.store.turnActive() || this.switching());

  protected readonly folderName = computed(() => {
    const parts = this.store.workspace().split(/[\\/]/).filter(Boolean);
    return parts.pop() ?? 'Choose a folder';
  });

  protected readonly folderParent = computed(() => {
    const path = this.store.workspace();
    if (!path) return 'no folder open';
    const parts = path.split(/[\\/]/).filter(Boolean);
    parts.pop();
    const parent = parts.length > 2 ? '…/' + parts.slice(-2).join('/') : '/' + parts.join('/');
    return parent.replace(/^\/Users\/[^/]+/, '~');
  });

  protected readonly coreLabel = computed(
    () =>
      ({ starting: 'Starting core…', ready: 'Core ready', stopped: 'Core stopped', error: 'Core unavailable' })[
        this.store.coreState()
      ],
  );
  protected readonly coreDot = computed(
    () =>
      ({ starting: 'dot-warning dot-live', ready: 'dot-success', stopped: 'dot-danger', error: 'dot-danger' })[
        this.store.coreState()
      ],
  );
  protected readonly coreDetail = computed(() => this.store.corePath() || this.coreLabel());

  protected useAgent(): void {
    if (this.store.chatMode()) void this.store.leaveChat();
  }

  protected useChat(): void {
    if (!this.store.chatMode()) void this.store.openChat();
  }

  /** Arrow keys switch mode, as in any radio group. */
  protected modeKeys(event: KeyboardEvent): void {
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
    event.preventDefault();
    if (this.modeLocked()) return;
    if (this.store.chatMode()) this.useAgent();
    else this.useChat();
  }

  protected async askDelete(sessionId: string, title: string): Promise<void> {
    if (this.store.turnActive()) return;
    const deleted = await this.confirm.ask({
      title: 'Delete this conversation?',
      message:
        'It disappears from the history and cannot be reopened. Workspace files are unchanged; its events stay in the audit log.',
      subject: title || 'Untitled',
      subjectIsText: true,
      confirmLabel: 'Delete conversation',
      tone: 'danger',
      action: () => this.store.deleteConversation(sessionId),
    });
    if (deleted) this.toast.show('Conversation deleted', 'success');
  }
}

/** The collapsed navigation: icons only, with tooltips. */
@Component({
  selector: 'pa-rail',
  imports: [Icon, Tooltip],
  template: `
    <nav class="rail" aria-label="Navigation" (keydown)="rove($event)">
      <div class="rail-head titlebar-row" data-tauri-drag-region="deep">
        <img class="brand-mark" src="/pwr-mark-96.png" alt="" width="22" height="22" />
      </div>
      <button
        class="icon-btn"
        (click)="layout.toggleLeft()"
        [attr.aria-expanded]="layout.left() === 'overlay'"
        aria-label="Show sidebar"
        paTooltip="Show sidebar"
        [paTooltipKeys]="keys.toggleSidebar"
      >
        <pa-icon name="panel-left" />
      </button>
      <button
        class="icon-btn"
        (click)="store.newConversation()"
        [disabled]="store.turnActive()"
        aria-label="New conversation"
        paTooltip="New conversation"
        [paTooltipKeys]="keys.newConversation"
      >
        <pa-icon name="square-pen" />
      </button>
      <span class="spacer"></span>
      <button
        class="icon-btn"
        (click)="ui.paletteOpen.set(true)"
        aria-label="Commands"
        paTooltip="Commands"
        [paTooltipKeys]="keys.palette"
      >
        <pa-icon name="command" />
      </button>
      <button
        class="icon-btn"
        (click)="ui.settingsOpen.set(true)"
        aria-label="Settings"
        paTooltip="Settings"
        [paTooltipKeys]="keys.settings"
      >
        <pa-icon name="settings" />
      </button>
    </nav>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Rail extends Navigation {
  protected rove(event: KeyboardEvent): void {
    roveFocus(event, event.currentTarget as HTMLElement, 'button');
  }
}
