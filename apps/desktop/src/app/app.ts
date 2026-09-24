import { ChangeDetectionStrategy, Component, HostListener, OnInit, computed, inject, signal } from '@angular/core';
import { AgentStore } from './core/agent.store';
import { inTauri } from './core/bridge';
import { LayoutService, RAIL } from './core/layout';
import { ThemeService } from './core/theme';
import { DialogStack, SHORTCUTS, UiStore, isMac } from './core/ui';
import { CommandPalette } from './ui/command-palette';
import { Composer } from './ui/composer';
import { ContextMeter } from './ui/context-meter';
import { Conversation } from './ui/conversation';
import { Inspector } from './ui/inspector';
import { Icon } from './ui/kit/icon';
import { ConfirmHost, Toasts } from './ui/kit/overlays';
import { Tooltip } from './ui/kit/tooltip';
import { ModelManager } from './ui/model-manager';
import { ModelPicker } from './ui/model-picker';
import { Permission } from './ui/permission';
import { Settings } from './ui/settings';
import { Rail, Sidebar } from './ui/sidebar';
import { WorkspaceTrust } from './ui/workspace-trust';

@Component({
  selector: 'app-root',
  imports: [
    Sidebar,
    Rail,
    Conversation,
    Composer,
    Inspector,
    Permission,
    ContextMeter,
    ModelPicker,
    ModelManager,
    WorkspaceTrust,
    Settings,
    CommandPalette,
    ConfirmHost,
    Toasts,
    Icon,
    Tooltip,
  ],
  templateUrl: './app.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App implements OnInit {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  private readonly ui = inject(UiStore);
  private readonly dialogs = inject(DialogStack);
  protected readonly isMac = isMac;
  protected readonly rail = RAIL;
  protected readonly keys = SHORTCUTS;
  /** In full screen macOS hides the traffic lights, so their space is given back. */
  protected readonly fullscreen = signal(false);

  /** The open conversation's title, as the history lists it. */
  protected readonly title = computed(() => {
    const id = this.store.sessionId();
    if (!id) return 'New conversation';
    return this.store.sessions().find((session) => session.sessionId === id)?.title || 'Conversation';
  });

  constructor() {
    // Created now so the theme is applied and followed from the first frame.
    inject(ThemeService);
  }

  ngOnInit(): void {
    void this.store.boot();
    void this.followFullscreen();
  }

  private async followFullscreen(): Promise<void> {
    if (!inTauri()) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const window = getCurrentWindow();
    const check = async () => this.fullscreen.set(await window.isFullscreen());
    await check();
    // Entering and leaving full screen both resize the window.
    await window.onResized(() => void check());
  }

  @HostListener('window:keydown', ['$event'])
  protected shortcut(event: KeyboardEvent): void {
    if (event.key === 'Escape' && !event.defaultPrevented && !this.dialogs.open) {
      if (this.layout.closeOverlays()) event.preventDefault();
      return;
    }
    const mod = isMac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
    if (!mod || event.repeat) return;
    const key = event.key.toLowerCase();
    const code = event.code;
    let handled = true;
    if (key === 'k' && !event.shiftKey && !event.altKey) this.ui.paletteOpen.update((open) => !open);
    else if (this.dialogs.open) handled = false;
    else if (key === 'n' && !event.shiftKey && !event.altKey) this.store.newConversation();
    else if (code === 'KeyB' && event.altKey) this.layout.toggleRight();
    else if (code === 'KeyB' && !event.shiftKey) this.layout.toggleLeft();
    else if (key === ',') this.ui.settingsOpen.set(true);
    else handled = false;
    if (handled) event.preventDefault();
  }
}
