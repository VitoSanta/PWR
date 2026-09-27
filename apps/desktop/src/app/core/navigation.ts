import { Injectable, computed, inject, signal } from '@angular/core';
import { AgentStore } from './agent.store';
import { inTauri } from './bridge';
import { ConfirmService, ToastService, isMac } from './ui';

/**
 * Where the person is and how they move: the mode, the workspace, the
 * conversations and the core's state. Shared by the sidebar and by the
 * switchers of the shells that have none, so every shell moves the same way.
 */
@Injectable({ providedIn: 'root' })
export class NavigationService {
  private readonly store = inject(AgentStore);
  private readonly confirm = inject(ConfirmService);
  private readonly toast = inject(ToastService);

  readonly switching = computed(() => this.store.switchingMode() || this.store.switchingWorkspace());
  readonly modeLocked = computed(() => this.store.turnActive() || this.switching());

  readonly folderName = computed(() => {
    const parts = this.store.workspace().split(/[\\/]/).filter(Boolean);
    return parts.pop() ?? 'Choose a folder';
  });

  readonly folderParent = computed(() => {
    const path = this.store.workspace();
    if (!path) return 'no folder open';
    const parts = path.split(/[\\/]/).filter(Boolean);
    parts.pop();
    const parent = parts.length > 2 ? '…/' + parts.slice(-2).join('/') : '/' + parts.join('/');
    return parent.replace(/^\/Users\/[^/]+/, '~');
  });

  /** The open conversation's title, as the history lists it. */
  readonly title = computed(() => {
    const id = this.store.sessionId();
    if (!id) return 'New conversation';
    return this.store.sessions().find((session) => session.sessionId === id)?.title || 'Conversation';
  });

  readonly coreLabel = computed(
    () =>
      ({ starting: 'Starting core…', ready: 'Core ready', stopped: 'Core stopped', error: 'Core unavailable' })[
        this.store.coreState()
      ],
  );
  readonly coreDot = computed(
    () =>
      ({ starting: 'dot-warning dot-live', ready: 'dot-success', stopped: 'dot-danger', error: 'dot-danger' })[
        this.store.coreState()
      ],
  );
  readonly coreDetail = computed(() => this.store.corePath() || this.coreLabel());

  useAgent(): void {
    if (this.store.chatMode()) void this.store.leaveChat();
  }

  useChat(): void {
    if (!this.store.chatMode()) void this.store.openChat();
  }

  toggleMode(): void {
    if (this.modeLocked()) return;
    if (this.store.chatMode()) this.useAgent();
    else this.useChat();
  }

  async askDelete(sessionId: string, title: string): Promise<void> {
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

/**
 * The window: whether it is full screen, which gives back the traffic
 * lights' space on macOS. Followed from the first frame.
 */
@Injectable({ providedIn: 'root' })
export class WindowState {
  readonly isMac = isMac;
  readonly fullscreen = signal(false);

  constructor() {
    void this.follow();
  }

  private async follow(): Promise<void> {
    if (!inTauri()) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const window = getCurrentWindow();
    const check = async () => this.fullscreen.set(await window.isFullscreen());
    await check();
    // Entering and leaving full screen both resize the window.
    await window.onResized(() => void check());
  }
}
