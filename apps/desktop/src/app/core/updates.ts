import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import { AgentStore } from './agent.store';
import { EditorStore } from './editor';
import { TerminalService } from './terminal';
import { ConfirmService } from './ui';
import { bridge, inTauri } from './bridge';
type Phase = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'installing' | 'installed';
@Injectable({ providedIn: 'root' })
export class UpdatesStore {
  private readonly agent = inject(AgentStore);
  private readonly editor = inject(EditorStore);
  private readonly terminals = inject(TerminalService);
  private readonly confirm = inject(ConfirmService);
  readonly phase = signal<Phase>('idle');
  readonly configured = signal(false);
  readonly currentVersion = signal('');
  readonly channel = signal('');
  readonly available = signal<{version: string; notes: string} | null>(null);
  readonly received = signal(0);
  readonly total = signal<number | null>(null);
  readonly message = signal('');
  readonly error = signal('');
  readonly auto = signal(localStorage.getItem('pwr.checkUpdates') === 'true');
  constructor() {
    const destroy = inject(DestroyRef);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    destroy.onDestroy(() => { disposed = true; unlisten?.(); });
    if (inTauri()) {
      void bridge.onUpdateProgress(p => { this.received.set(p.received); this.total.set(p.total); }).then(off => { if (disposed) off(); else unlisten = off; }).catch(e => this.error.set(String(e)));
      void bridge.updateStatus().then(s => {
        if (disposed) return;
        this.configured.set(s.configured); this.currentVersion.set(s.currentVersion); this.channel.set(s.channel);
        if (s.configured && this.auto() && !this.agent.turnActive()) void this.check();
      }).catch(e => this.error.set(String(e)));
    }
  }
  setAuto(value: boolean): void { this.auto.set(value); localStorage.setItem('pwr.checkUpdates', String(value)); }
  busy(): boolean { return ['checking', 'downloading', 'installing'].includes(this.phase()); }
  blocked(): string {
    if (this.agent.turnActive() || this.agent.switchingWorkspace() || this.agent.engineInstalling()) return 'Wait for the current activity to finish.';
    if (this.editor.hasUnsavedChanges()) return 'Save or close unsaved files in all workspaces first.';
    if (this.terminals.sessions().length) return 'Close all terminals before installing.';
    return '';
  }
  async check(): Promise<void> {
    if (this.busy() || this.phase() === 'installed' || !this.configured()) return;
    this.phase.set('checking'); this.error.set(''); this.message.set(''); this.available.set(null);
    try {
      const update = await bridge.updateCheck(); this.available.set(update);
      this.phase.set(update ? 'available' : 'idle');
      this.message.set(update ? '' : 'PWR is up to date for this channel.');
    } catch (e) { this.error.set(String(e)); this.phase.set('idle'); }
  }
  async download(): Promise<void> {
    if (this.phase() !== 'available') return;
    this.phase.set('downloading'); this.received.set(0); this.total.set(null); this.error.set('');
    try { await bridge.updateDownload(); this.phase.set('ready'); }
    catch (e) { this.error.set(String(e)); this.phase.set('available'); }
  }
  async install(): Promise<void> {
    if (this.phase() !== 'ready') return;
    this.error.set('');
    if (this.blocked()) { this.error.set(this.blocked()); return; }
    if (!await this.confirm.ask({ title: 'Install update and restart?', message: 'PWR will stop the workspace and its background services, install the verified update and restart.', confirmLabel: 'Install and restart' })) return;
    if (this.phase() !== 'ready') return;
    if (this.blocked()) { this.error.set(this.blocked()); return; }
    this.phase.set('installing');
    try {
      await bridge.stop();
      if (this.blocked()) throw new Error(this.blocked());
      await bridge.updateInstall(); this.phase.set('installed');
      await this.restart();
    } catch (e) { this.error.set(String(e)); if (this.phase() !== 'installed') this.phase.set('ready'); }
  }
  async restart(): Promise<void> {
    if (this.phase() !== 'installed') return;
    if (this.editor.hasUnsavedChanges()) { this.error.set('Save or close unsaved files before restarting.'); return; }
    try { await bridge.updateRestart(); } catch (e) { this.error.set(String(e)); }
  }
}
