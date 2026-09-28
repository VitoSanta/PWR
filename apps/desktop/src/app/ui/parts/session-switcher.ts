import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { shortDate } from '../../core/format';
import { NavigationService } from '../../core/navigation';
import { SHORTCUTS, UiStore, shortcut } from '../../core/ui';
import { Icon } from '../kit/icon';
import { Popover } from '../kit/popover';
import { Tooltip } from '../kit/tooltip';

/**
 * The sidebar folded into one control, for the shells that have no sidebar:
 * the open conversation's title, opening onto the mode, the workspace, a
 * new conversation and the history. ⌘B opens it.
 */
@Component({
  selector: 'pa-session-switcher',
  imports: [Icon, Popover, Tooltip],
  template: `
    <button
      #trigger
      class="switcher-trigger"
      [class.is-icon]="compact()"
      (click)="ui.sessionsOpen.set(!ui.sessionsOpen())"
      [attr.aria-expanded]="ui.sessionsOpen()"
      aria-haspopup="dialog"
      [attr.aria-label]="'Conversations: ' + nav.title()"
      [paTooltip]="compact() ? 'Conversations' : null"
      [paTooltipKeys]="keys.toggleSidebar"
    >
      @if (compact()) {
        <pa-icon name="message" />
      } @else {
        <span class="switcher-text">
          <span class="switcher-title truncate">{{ nav.title() }}</span>
          <span class="switcher-where truncate">{{ store.chatMode() ? 'Chat · no workspace' : nav.folderName() }}</span>
        </span>
        <pa-icon name="chevron-down" [size]="14" />
      }
    </button>

    @if (ui.sessionsOpen()) {
      <pa-popover
        [anchor]="trigger"
        [side]="side()"
        [anchorAlign]="align()"
        width="380px"
        ariaLabel="Conversations"
        (closed)="ui.sessionsOpen.set(false)"
        animate.leave="anim-pop-out"
      >
        <div class="popover-head">
          <h2 class="popover-title">Conversations</h2>
          <button class="icon-btn icon-btn-sm" (click)="ui.sessionsOpen.set(false)" aria-label="Close">
            <pa-icon name="x" [size]="16" />
          </button>
        </div>
        <div class="popover-section">
          <div class="segmented segmented-block" role="radiogroup" aria-label="Conversation mode" [attr.aria-busy]="nav.switching()">
            <button
              role="radio"
              [attr.aria-checked]="!store.chatMode()"
              (click)="nav.useAgent()"
              [disabled]="nav.modeLocked()"
            >
              <pa-icon name="terminal" [size]="14" /> Agent
            </button>
            <button
              role="radio"
              [attr.aria-checked]="store.chatMode()"
              (click)="nav.useChat()"
              [disabled]="nav.modeLocked() || (!store.chatMode() && !store.chatHome())"
            >
              <pa-icon name="message" [size]="14" /> Chat
            </button>
          </div>
          @if (store.chatMode()) {
            <div class="workspace-row">
              <pa-icon name="lock" [size]="16" />
              <span class="workspace-text">
                <span class="workspace-name">No workspace</span>
                <span class="workspace-path truncate">read-only · saved globally</span>
              </span>
            </div>
          } @else {
            <button class="workspace-row" (click)="store.chooseWorkspace()" [disabled]="nav.switching()" aria-label="Change workspace folder">
              <pa-icon name="folder" [size]="16" />
              <span class="workspace-text">
                <span class="workspace-name truncate">{{ nav.folderName() }}</span>
                <span class="workspace-path truncate">{{ nav.folderParent() }}</span>
              </span>
              <pa-icon class="workspace-change" name="chevron-right" [size]="16" />
            </button>
          }
          <button class="btn btn-block new-conversation" (click)="start()" [disabled]="store.turnActive()">
            <pa-icon name="square-pen" [size]="16" />
            New conversation
            <span class="kbd" aria-hidden="true">{{ shortcut(keys.newConversation) }}</span>
          </button>
        </div>
        <div class="popover-body switcher-list">
          <ul class="session-list" aria-label="Conversations">
            @for (session of store.sessions(); track session.sessionId) {
              <li class="session-row" [class.is-current]="session.sessionId === store.sessionId()">
                <button
                  class="session"
                  (click)="open(session.sessionId)"
                  [disabled]="store.turnActive()"
                  [attr.aria-current]="session.sessionId === store.sessionId() ? 'page' : null"
                  [attr.title]="session.title || 'Untitled'"
                >
                  <span class="session-title truncate">{{ session.title || 'Untitled' }}</span>
                  <span class="session-meta">{{ date(session.updatedAt) }}</span>
                </button>
                <button
                  class="icon-btn icon-btn-sm icon-btn-danger session-delete"
                  (click)="nav.askDelete(session.sessionId, session.title)"
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
        </div>
      </pa-popover>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SessionSwitcher {
  protected readonly store = inject(AgentStore);
  protected readonly nav = inject(NavigationService);
  protected readonly ui = inject(UiStore);
  protected readonly keys = SHORTCUTS;
  protected readonly shortcut = shortcut;
  protected readonly date = shortDate;

  /** An icon, for a rail; otherwise the title and where it is. */
  readonly compact = input(false);
  readonly side = input<'bottom' | 'top'>('bottom');
  readonly align = input<'start' | 'end'>('start');

  protected start(): void {
    this.store.newConversation();
    this.ui.sessionsOpen.set(false);
  }

  protected async open(sessionId: string): Promise<void> {
    this.ui.sessionsOpen.set(false);
    await this.store.resume(sessionId);
  }
}
