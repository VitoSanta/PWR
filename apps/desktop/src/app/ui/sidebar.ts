import { ChangeDetectionStrategy, Component, computed, effect, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { bridge } from '../core/bridge';
import { shortDate } from '../core/format';
import { LEFT, LayoutService } from '../core/layout';
import { ModelsStore } from '../core/models.store';
import { NavigationService } from '../core/navigation';
import { PersonalStore } from '../core/personal.store';
import { SHORTCUTS, UiStore, shortcut } from '../core/ui';
import { Icon } from './kit/icon';
import { ResizeHandle } from './kit/resize-handle';
import { Tooltip } from './kit/tooltip';

/**
 * The sidebar: a new conversation, this project's conversations -- the
 * running one says so -- the other projects, and at the foot the models
 * and the person, who opens Settings.
 */
@Component({
  selector: 'pa-sidebar',
  imports: [Icon, Tooltip, ResizeHandle],
  template: `
    <nav class="sidebar" aria-label="Conversations and projects">
      <header class="sidebar-head titlebar-row" data-tauri-drag-region="deep">
        <span class="wordmark" data-tauri-drag-region="deep">pwr</span>
        <span class="spacer" data-tauri-drag-region="deep"></span>
        <button
          class="sidebar-key"
          (click)="ui.paletteOpen.set(true)"
          aria-label="Commands"
          paTooltip="Commands"
        >
          {{ shortcut(keys.palette) }}
        </button>
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
        <button class="btn btn-ink btn-block new-conversation" (click)="store.newConversation()" [disabled]="store.turnActive()">
          <pa-icon name="plus" [size]="16" />
          New conversation
          <span class="kbd kbd-on-ink" aria-hidden="true">{{ shortcut(keys.newConversation) }}</span>
        </button>
        @if (nav.switching()) {
          <p class="sidebar-status" role="status"><span class="spinner spinner-sm" aria-hidden="true"></span>Opening workspace…</p>
        }
      </div>

      <div class="sidebar-scroll">
        <section class="sidebar-section" aria-labelledby="sessions-label">
          <div class="sidebar-label-row">
            <h2 class="sidebar-label truncate" id="sessions-label" [attr.title]="store.chatMode() ? null : store.workspace()">
              {{ store.chatMode() ? 'Chat' : nav.folderName() }}
            </h2>
            @if (!store.chatMode()) {
              <button
                class="icon-btn icon-btn-xs"
                (click)="store.chooseWorkspace()"
                [disabled]="nav.switching()"
                aria-label="Open another folder"
                paTooltip="Open another folder"
              >
                <pa-icon name="folder" [size]="14" />
              </button>
            }
          </div>
          <ul class="session-list">
            @for (session of store.sessions(); track session.sessionId) {
              @let current = session.sessionId === store.sessionId();
              <li class="session-row" [class.is-current]="current">
                <button
                  class="session"
                  (click)="open(session.sessionId)"
                  (keydown.delete)="nav.askDelete(session.sessionId, session.title)"
                  [disabled]="store.turnActive() && !current"
                  [attr.aria-current]="current ? 'page' : null"
                  [attr.title]="session.title || 'Untitled'"
                >
                  <span class="session-title truncate">{{ session.title || 'Untitled' }}</span>
                  @if (current && store.turnActive()) {
                    <span class="session-meta is-running"><span class="dot dot-running" aria-hidden="true"></span>running · {{ running() }}</span>
                  } @else {
                    <span class="session-meta">{{ date(session.updatedAt) }}</span>
                  }
                </button>
                <button
                  class="icon-btn icon-btn-xs icon-btn-danger session-delete"
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
        </section>

        <section class="sidebar-section" aria-labelledby="projects-label">
          <h2 class="sidebar-label" id="projects-label">Other projects</h2>
          <ul class="project-list">
            @for (project of otherProjects(); track project.path) {
              <li>
                <button class="project-row" (click)="openProject(project.path)" [disabled]="nav.modeLocked()" [attr.title]="project.path">
                  <span class="truncate">{{ project.name }}</span>
                </button>
              </li>
            }
            @if (store.chatMode()) {
              <li>
                <button class="project-row is-quiet" (click)="nav.useAgent()" [disabled]="nav.modeLocked()">
                  <pa-icon name="folder" [size]="14" /> Back to a workspace
                </button>
              </li>
            } @else if (store.chatHome()) {
              <li>
                <button class="project-row is-quiet" (click)="nav.useChat()" [disabled]="nav.modeLocked()" paTooltip="Talk to the model without a workspace: it reads only what you attach">
                  <pa-icon name="message" [size]="14" /> Chat, no workspace
                </button>
              </li>
            }
            <li>
              <button class="project-row is-quiet" (click)="store.chooseWorkspace()" [disabled]="nav.switching()">
                <pa-icon name="plus" [size]="14" /> Open a folder…
              </button>
            </li>
          </ul>
        </section>
      </div>

      <footer class="sidebar-foot">
        @if (store.coreState() !== 'ready') {
          <span class="core-status" [paTooltip]="nav.coreDetail()">
            <span class="dot" [class]="'dot ' + nav.coreDot()" aria-hidden="true"></span>
            <span class="truncate">{{ nav.coreLabel() }}</span>
          </span>
        }
        <button class="foot-row" (click)="models.show()">
          <span>Models</span>
          <span class="foot-count num">{{ store.models().length }}</span>
        </button>
        <button class="foot-row person" (click)="ui.settingsOpen.set(true)" [paTooltip]="'Settings'" [paTooltipKeys]="keys.settings">
          <span class="avatar" aria-hidden="true">{{ initial() }}</span>
          <span class="person-name truncate">{{ name() }}</span>
          <span class="sidebar-hint" aria-hidden="true">{{ shortcut(keys.settings) }}</span>
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
export class Sidebar {
  protected readonly store = inject(AgentStore);
  protected readonly nav = inject(NavigationService);
  protected readonly layout = inject(LayoutService);
  protected readonly ui = inject(UiStore);
  protected readonly models = inject(ModelsStore);
  private readonly personal = inject(PersonalStore);
  protected readonly keys = SHORTCUTS;
  protected readonly shortcut = shortcut;
  protected readonly bounds = LEFT;
  protected readonly date = shortDate;

  private readonly now = signal(Date.now());

  /** The person, as Settings → Profile names them. */
  protected readonly name = computed(() => this.personal.profile().name?.trim() || 'You');
  protected readonly initial = computed(() => this.name().charAt(0).toUpperCase());

  /** Projects with a wiki other than the one open, newest first. */
  protected readonly otherProjects = computed(() => {
    const here = this.store.workspace();
    return this.personal
      .projects()
      .filter((project) => project.path !== here)
      .slice()
      .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
      .slice(0, 6);
  });

  /** How long the running turn has been working. */
  protected readonly running = computed(() => {
    const since = [...this.store.timeline()].reverse().find((entry) => entry.kind === 'user')?.at;
    const seconds = since ? Math.max(0, Math.round((this.now() - since) / 1000)) : 0;
    return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
  });

  constructor() {
    setInterval(() => {
      if (this.store.turnActive()) this.now.set(Date.now());
    }, 1000);
    // The person's name and the projects, once the core can say them, and
    // again in each workspace.
    effect(() => {
      if (this.store.coreState() !== 'ready') return;
      this.store.workspace();
      void this.personal.load();
    });
  }

  protected open(sessionId: string): void {
    if (sessionId !== this.store.sessionId()) void this.store.resume(sessionId);
  }

  /** Another project: asked about first if this Mac has not trusted it, as a picked folder is. */
  protected async openProject(path: string): Promise<void> {
    try {
      if (!(await bridge.workspaceIsTrusted(path))) {
        this.store.workspaceTrustError.set('');
        this.store.pendingWorkspaceTrust.set(path);
        return;
      }
      await this.store.openWorkspace(path);
    } catch (error) {
      this.store.coreError.set(String(error));
    }
  }
}
