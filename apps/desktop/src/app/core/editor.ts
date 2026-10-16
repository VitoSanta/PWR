import { DestroyRef, Injectable, computed, effect, inject, signal, untracked } from '@angular/core';
import { AgentStore } from './agent.store';
import { WorkbenchStore } from './workbench';
import { ConfirmService } from './ui';

export interface EditorDocument {
  path: string; text: string; saved: string; hash: string; conflict: boolean;
  readonly?: boolean; binary?: boolean;
}
export interface EditorEntry { path: string; name: string; dir: boolean; bytes: number; depth?: number }
@Injectable({ providedIn: 'root' })
export class EditorStore {
  private readonly agent = inject(AgentStore);
  private readonly work = inject(WorkbenchStore);
  private readonly confirm = inject(ConfirmService);
  readonly documents = signal<EditorDocument[]>([]);
  readonly activePath = signal('');
  readonly active = computed(() => this.documents().find(doc => doc.path === this.activePath()) ?? null);
  readonly entries = signal<EditorEntry[]>([]);
  readonly expanded = signal<string[]>([]);
  readonly error = signal('');
  readonly saving = signal(false);
  readonly loading = signal(false);
  readonly lastTrash = signal<{ path: string; token: string; cwd: string } | null>(null);
  readonly selection = signal<{ path: string; text: string; firstLine: number; lastLine: number } | null>(null);
  private readonly buffers = new Map<string, { documents: EditorDocument[]; active: string }>();
  private cwd = '';
  private epoch = 0;
  private openVersion = 0;
  private listVersion = 0;
  private refreshing = false;
  constructor() {
    effect(() => {
      const cwd = this.agent.workspace();
      const ready = this.agent.coreState() === 'ready';
      untracked(() => {
        if (cwd !== this.cwd) {
          this.buffers.set(this.cwd, { documents: this.documents(), active: this.activePath() });
          this.cwd = cwd; this.epoch++;
          const saved = this.buffers.get(cwd);
          this.documents.set(saved?.documents ?? []); this.activePath.set(saved?.active ?? '');
          this.entries.set([]); this.expanded.set([]); this.selection.set(null); this.error.set('');
        }
        if (ready && cwd) void this.list();
      });
    });
    effect(() => {
      const path = this.work.fileRequest();
      if (path) untracked(() => { this.work.fileRequest.set(null); void this.open(path); });
    });
    // Poll disk changes without replacing unsaved text. Buffers outlive cards.
    const timer = setInterval(() => { if (this.agent.coreState() === 'ready') void this.refresh(); }, 2500);
    inject(DestroyRef).onDestroy(() => clearInterval(timer));
    this.agent.workspaceHandlers.set('editor', async (operation, args) => {
      if (operation === 'guard') {
        const dirty = this.documents().filter(doc => doc.text !== doc.saved);
        const path = String(args.path ?? '');
        if (dirty.some(doc => !path || doc.path === path || doc.path.startsWith(path + '/'))) {
          throw new Error('Unsaved editor changes overlap this action. Ask the engineer to save or discard them before changing files.');
        }
        return { allowed: true };
      }
      this.work.show('files');
      if (operation === 'open') { await this.open(String(args.path ?? '')); return this.snapshot(); }
      if (operation === 'snapshot') return this.snapshot();
      throw new Error('Editor supports snapshot and open. Modify saved files with the normal checked file tools.');
    });
  }
  dirty(doc: EditorDocument): boolean { return doc.text !== doc.saved; }
  edit(path: string, text: string): void {
    this.documents.update(docs => docs.map(doc => doc.path === path ? { ...doc, text } : doc));
  }
  snapshot(): unknown {
    const doc = this.active();
    return { active: doc ? { path: doc.path, dirty: this.dirty(doc), conflict: doc.conflict, hash: doc.hash } : null,
      selection: this.selection(), open: this.documents().map(doc => ({ path: doc.path, dirty: this.dirty(doc) })),
      note: 'Editor buffers are user data. Unsaved text must not be overwritten.' };
  }
  async open(path: string): Promise<void> {
    if (!path) return;
    const version = ++this.openVersion;
    const existing = this.documents().find(doc => doc.path === path);
    if (existing) { this.activePath.set(path); return; }
    const cwd = this.cwd, epoch = this.epoch;
    try {
      const reply = await this.agent.call('_pwr/file', { cwd, path });
      if (epoch !== this.epoch) return;
      if (reply.binary) throw new Error('This is a binary file. Open it in its own application.');
      const doc: EditorDocument = { path, text: reply.text ?? '', saved: reply.text ?? '', hash: reply.hash,
        readonly: reply.truncated || !reply.hash, conflict: false };
      if (!this.documents().some(item => item.path === path)) this.documents.update(docs => [...docs, doc]);
      if (version === this.openVersion) this.activePath.set(path); this.error.set('');
    } catch (error) { if (cwd === this.cwd) this.error.set(String(error)); }
  }
  async list(): Promise<void> {
    const cwd = this.cwd, epoch = this.epoch, version = ++this.listVersion;
    this.loading.set(true);
    try {
      const folders = ['', ...this.expanded()];
      const responses = await Promise.all(folders.map(path => this.agent.call('_pwr/files', { cwd, path })));
      if (epoch !== this.epoch || version !== this.listVersion) return;
      const children = new Map(folders.map((path, i) => [path, responses[i].entries as EditorEntry[]]));
      const shown: EditorEntry[] = [];
      const visit = (folder: string, depth: number) => {
        for (const entry of children.get(folder) ?? []) {
          if (['.pwr', '.git', '.pwr-scratch'].includes(entry.name)) continue;
          shown.push({ ...entry, depth });
          if (entry.dir && this.expanded().includes(entry.path)) visit(entry.path, depth + 1);
        }
      };
      visit('', 0); this.entries.set(shown); this.error.set('');
    } catch (error) { if (epoch === this.epoch && version === this.listVersion) this.error.set(String(error)); }
    finally { if (epoch === this.epoch && version === this.listVersion) this.loading.set(false); }
  }
  async toggle(entry: EditorEntry): Promise<void> {
    if (!entry.dir) return this.open(entry.path);
    this.expanded.update(paths => paths.includes(entry.path) ? paths.filter(path => path !== entry.path && !path.startsWith(entry.path + '/')) : [...paths, entry.path]);
    await this.list();
  }
  async save(): Promise<void> {
    const doc = this.active(), cwd = this.cwd;
    if (!doc || doc.readonly || this.saving()) return;
    this.saving.set(true); this.error.set('');
    try {
      const reply = await this.agent.call('_pwr/editor', { cwd, operation: 'save', path: doc.path, text: doc.text, hash: doc.hash });
      if (cwd !== this.cwd) return;
      this.documents.update(docs => docs.map(item => item.path === doc.path ? { ...item, saved: doc.text, hash: reply.hash, conflict: false } : item));
    } catch (error) { if (cwd === this.cwd) this.error.set(String(error)); }
    finally { this.saving.set(false); }
  }
  async close(doc: EditorDocument): Promise<void> {
    if (this.dirty(doc) && !await this.confirm.ask({ title: 'Discard unsaved changes?', subject: doc.path, confirmLabel: 'Discard changes', tone: 'danger' })) return;
    const docs = this.documents().filter(item => item.path !== doc.path);
    this.documents.set(docs);
    if (this.activePath() === doc.path) this.activePath.set(docs.at(-1)?.path ?? '');
  }
  async reload(): Promise<void> {
    const doc = this.active(); if (!doc) return;
    if (this.dirty(doc) && !await this.confirm.ask({ title: 'Reload from disk?', message: 'Unsaved changes in this file will be discarded.', confirmLabel: 'Reload', tone: 'danger' })) return;
    this.documents.update(docs => docs.filter(item => item.path !== doc.path));
    await this.open(doc.path);
  }
  async mutate(operation: string, path: string, to?: string): Promise<void> {
    const cwd = this.cwd;
    const affected = this.documents().filter(doc => doc.path === path || doc.path.startsWith(path + '/'));
    if (affected.some(doc => this.dirty(doc))) throw new Error('Save or close unsaved files before moving or removing this path.');
    const doc = this.documents().find(doc => doc.path === path);
    const reply = await this.agent.call('_pwr/editor', { cwd, operation, path, to, text: '', hash: doc?.hash });
    if (cwd !== this.cwd) return;
    if (operation === 'trash') {
      this.lastTrash.set({ path, token: reply.restoreToken, cwd });
      this.documents.update(docs => docs.filter(item => !affected.includes(item)));
      this.expanded.update(paths => paths.filter(item => item !== path && !item.startsWith(path + '/')));
      if (!this.documents().some(doc => doc.path === this.activePath())) this.activePath.set(this.documents().at(-1)?.path ?? '');
    } else if (operation === 'rename') {
      this.expanded.update(paths => paths.map(item => item === path || item.startsWith(path + '/') ? to + item.slice(path.length) : item));
      this.documents.update(docs => docs.map(item => affected.includes(item) ? { ...item, path: to + item.path.slice(path.length) } : item));
      if (this.activePath() === path || this.activePath().startsWith(path + '/')) this.activePath.set(to + this.activePath().slice(path.length));
    }
    await this.list();
    if (operation === 'create_file') await this.open(path);
  }
  async restore(): Promise<void> {
    const last = this.lastTrash(); if (!last || last.cwd !== this.cwd) return;
    await this.agent.call('_pwr/editor', { cwd: this.cwd, operation: 'restore', path: last.path, restoreToken: last.token });
    this.lastTrash.set(null); await this.list();
  }
  ask(): void {
    const selected = this.selection(), doc = this.active(); if (!doc) return;
    const reference = selected?.text ? `${doc.path}:${selected.firstLine}-${selected.lastLine}\n\n${selected.text.slice(0, 16000)}` : doc.path;
    this.agent.composerDraft.set(`Regarding ${reference}\n\n`);
  }
  private async refresh(): Promise<void> {
    if (this.refreshing || !this.documents().length) return;
    this.refreshing = true;
    const cwd = this.cwd, epoch = this.epoch;
    try {
      for (const original of this.documents()) {
        const reply = await this.agent.call('_pwr/file', { cwd, path: original.path });
        if (epoch !== this.epoch) return;
        this.documents.update(docs => docs.map(doc => {
          if (doc.path !== original.path || !reply.hash || reply.hash === doc.hash) return doc;
          return this.dirty(doc) ? { ...doc, conflict: true } : { ...doc, text: reply.text, saved: reply.text, hash: reply.hash, conflict: false };
        }));
      }
    } catch { /* A deleted or renamed file stays in its buffer until reviewed. */ }
    finally { this.refreshing = false; }
  }
}
