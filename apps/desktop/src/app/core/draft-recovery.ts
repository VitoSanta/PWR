import { Injectable, signal } from '@angular/core';
import { bridge, inTauri } from './bridge';
import type { EditorDocument } from './editor';
export interface DraftSnapshot { version: number; workspace: string; active: string; documents: EditorDocument[] }
@Injectable({ providedIn: 'root' })
export class DraftRecovery {
  readonly enabled = inTauri();
  readonly error = signal('');
  private tail: Promise<void> = Promise.resolve();
  private readonly blocked = new Set<string>();
  async read(workspace: string): Promise<DraftSnapshot | null> {
    if (!this.enabled) return null;
    try { return await bridge.draftsRead(workspace); }
    catch (e) { this.blocked.add(workspace); this.error.set(`Editor recovery could not be read. The existing copy is retained: ${e}`); return null; }
  }
  write(workspace: string, snapshot: DraftSnapshot | null): Promise<void> {
    if (!this.enabled) return Promise.resolve();
    const task = this.tail.then(async () => {
      if (this.blocked.has(workspace)) {
        // A clean quit/discard must preserve an unreadable record, not trap the
        // user in the app or silently overwrite the original recovery data.
        if (snapshot === null) return;
        throw new Error('The existing recovery record needs attention before it can be replaced.');
      }
      await bridge.draftsWrite(workspace, snapshot);
    });
    this.tail = task.catch(e => { this.error.set(`Editor recovery could not be saved: ${e}`); });
    return task;
  }
  flush(): Promise<void> { return this.tail; }
}
