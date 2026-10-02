import { ChangeDetectionStrategy, Component, HostListener, OnInit, inject } from '@angular/core';
import { AgentStore } from './core/agent.store';
import { bridge, inTauri } from './core/bridge';
import { LayoutService } from './core/layout';
import { WindowState } from './core/navigation';
import { ThemeService } from './core/theme';
import { ActivityStore } from './core/activity';
import { WorkbenchStore } from './core/workbench';
import { DialogStack, ToastService, UiStore, isMac } from './core/ui';
import { CommandPalette } from './ui/command-palette';
import { ConfirmHost, Toasts } from './ui/kit/overlays';
import { ModelManager } from './ui/model-manager';
import { Permission } from './ui/permission';
import { EngineSetup } from './ui/engine-setup';
import { Settings } from './ui/settings';
import { WorkspaceTrust } from './ui/workspace-trust';
import { Shell } from './ui/shell';

/**
 * The app: shared stores, dialogs and shortcuts around the shell.
 */
@Component({
  selector: 'app-root',
  imports: [
    Shell,
    Permission,
    ModelManager,
    WorkspaceTrust,
    Settings,
    EngineSetup,
    CommandPalette,
    ConfirmHost,
    Toasts,
  ],
  templateUrl: './app.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App implements OnInit {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  /** Followed from the first frame: in full screen macOS gives the traffic lights' space back. */
  protected readonly win = inject(WindowState);
  private readonly ui = inject(UiStore);
  private readonly dialogs = inject(DialogStack);
  private readonly toast = inject(ToastService);
  private readonly work = inject(WorkbenchStore);

  constructor() {
    // Created now so the theme is applied and followed from the first frame.
    inject(ThemeService);
    // Listening from the start, so background work begun before the
    // Activity card opens is still shown in it.
    inject(ActivityStore);
  }

  ngOnInit(): void {
    void this.store.boot();
  }

  /**
   * Web links open in the browser. In the app's webview a new-tab link does
   * nothing, and any other would replace the whole interface -- the Hub
   * links, and links inside what a model writes.
   */
  @HostListener('document:click', ['$event'])
  protected externalLink(event: MouseEvent): void {
    const link = (event.target as Element | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
    if (!link || event.defaultPrevented || event.button !== 0) return;
    const url = new URL(link.href, location.href);
    if (!/^https?:$/.test(url.protocol) || url.origin === location.origin) return;
    event.preventDefault();
    if (inTauri()) {
      bridge.openExternal(url.href).catch((error) => this.toast.show(String(error), 'danger'));
    } else {
      window.open(url.href, '_blank', 'noopener');
    }
  }

  @HostListener('window:keydown', ['$event'])
  protected shortcut(event: KeyboardEvent): void {
    if (event.key === 'Escape' && !event.defaultPrevented && !this.dialogs.open) {
      if (this.layout.closeOverlays()) event.preventDefault();
      return;
    }
    // The workbench's Control shortcuts: ⌃` and ⌃⇧G.
    if (event.ctrlKey && !event.metaKey && !event.altKey && !event.repeat && !this.dialogs.open) {
      if (event.code === 'Backquote' && !event.shiftKey) {
        event.preventDefault();
        this.work.toggle('terminal');
        return;
      }
      if (event.code === 'KeyG' && event.shiftKey) {
        event.preventDefault();
        this.work.toggle('review');
        return;
      }
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
    else if (code === 'KeyT' && event.shiftKey && !event.altKey) this.work.toggle('browser');
    else if (code === 'KeyP' && !event.shiftKey && !event.altKey) this.work.toggle('files');
    else handled = false;
    if (handled) event.preventDefault();
  }
}
