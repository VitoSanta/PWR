import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { AgentStore } from './core/agent.store';
import { Composer } from './ui/composer';
import { ContextMeter } from './ui/context-meter';
import { Conversation } from './ui/conversation';
import { Inspector } from './ui/inspector';
import { ModelManager } from './ui/model-manager';
import { ModelPicker } from './ui/model-picker';
import { Permission } from './ui/permission';
import { Sidebar } from './ui/sidebar';
import { WorkspaceTrust } from './ui/workspace-trust';

@Component({
  selector: 'app-root',
  imports: [Sidebar, Conversation, Composer, Inspector, Permission, ContextMeter, ModelPicker, ModelManager, WorkspaceTrust],
  templateUrl: './app.html',
  styleUrl: './app.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App implements OnInit {
  protected readonly store = inject(AgentStore);
  protected readonly isMac = typeof navigator !== 'undefined' && /Macintosh|Mac OS X/.test(navigator.userAgent);
  protected readonly leftWidth = signal(this.savedWidth('left', 272));
  protected readonly rightWidth = signal(this.savedWidth('right', 360));

  protected setLeftWidth(width: number): void {
    this.leftWidth.set(width);
    localStorage.setItem('pwr:left-width', String(width));
  }

  protected setRightWidth(width: number): void {
    this.rightWidth.set(width);
    localStorage.setItem('pwr:right-width', String(width));
  }

  private savedWidth(side: 'left' | 'right', fallback: number): number {
    if (typeof localStorage === 'undefined') return fallback;
    const width = Number(localStorage.getItem(`pwr:${side}-width`));
    return Number.isFinite(width) && width >= 230 && width <= 500 ? width : fallback;
  }

  ngOnInit(): void {
    void this.store.boot();
  }
}
