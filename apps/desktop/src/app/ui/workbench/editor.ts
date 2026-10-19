import { AfterViewInit, ChangeDetectionStrategy, Component, ElementRef, OnDestroy, effect, inject, signal, untracked, viewChild } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { FormsModule } from '@angular/forms';
import { EditorStore, EditorDocument } from '../../core/editor';
import { AgentStore } from '../../core/agent.store';
import { ThemeService } from '../../core/theme';
import { ConfirmService } from '../../core/ui';
import { Diff } from '../diff';
import { Icon } from '../kit/icon';
import { Action, EmptyState, SearchField, ToolBar, ToolTab } from '../kit/tool';
import { loadEditorStyles } from '../../core/editor-styles';
import type * as Monaco from 'monaco-editor/editor/editor.api.js';

@Component({
  selector: 'pa-code-editor',
  template: `<div #surface class="code-editor-surface"></div>@if (error()) { <p role="alert">{{ error() }}</p> }`,
  host: { class: 'code-editor' }, changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CodeEditor implements AfterViewInit, OnDestroy {
  private readonly store = inject(EditorStore);
  private readonly theme = inject(ThemeService);
  private readonly surface = viewChild.required<ElementRef<HTMLElement>>('surface');
  protected readonly error = signal('');
  private ready = signal(false);
  private disposed = false;
  private monaco?: typeof Monaco;
  private editor?: Monaco.editor.IStandaloneCodeEditor;
  private models = new Map<string, Monaco.editor.ITextModel>();
  private listeners: Monaco.IDisposable[] = [];
  private updating = false;
  private appliedTheme = '';
  private viewStates = new Map<string, Monaco.editor.ICodeEditorViewState>();
  private shownPath = '';
  constructor() {
    effect(() => {
      const doc = this.store.active();
      const docs = this.store.documents();
      const theme = this.theme.theme();
      const palette = this.theme.palette();
      if (!this.ready()) return;
      untracked(() => this.show(doc, docs, theme, palette));
    });
  }
  async ngAfterViewInit(): Promise<void> {
    try {
      (globalThis as any).MonacoEnvironment = { getWorker: () => new Worker(new URL('./editor.worker', import.meta.url), { type: 'module' }) };
      await loadEditorStyles();
      if (this.disposed) return;
      const monaco = await import('monaco-editor/editor/editor.api.js');
      await import('monaco-editor/basic-languages/monaco.contribution.js');
      if (this.disposed) return;
      this.monaco = monaco;
      const editor = monaco.editor.create(this.surface().nativeElement, { automaticLayout: true, minimap: { enabled: false }, fontSize: 13,
        fontFamily: '"SF Mono", Menlo, Monaco, Consolas, monospace', fontLigatures: false, lineHeight: 22,
        padding: { top: 10, bottom: 10 }, renderLineHighlight: 'line', bracketPairColorization: { enabled: true },
        scrollBeyondLastLine: false, tabSize: 2, wordWrap: 'off', accessibilitySupport: 'auto', ariaLabel: 'Source code editor' });
      this.editor = editor;
      this.listeners.push(editor.onDidChangeModelContent(() => {
        if (!this.updating) {
          const path = this.store.activePath(); this.store.edit(path, this.editor!.getValue());
        }
      }));
      this.listeners.push(editor.onDidChangeCursorSelection(() => {
        const selection = this.editor!.getSelection(), model = this.editor!.getModel();
        this.store.selection.set(selection && model ? { path: this.store.activePath(), text: model.getValueInRange(selection), firstLine: selection.startLineNumber, lastLine: selection.endLineNumber } : null);
      }));
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => void this.store.save());
      this.ready.set(true);
      void document.fonts.ready.then(() => {
        if (!this.disposed) { monaco.editor.remeasureFonts(); editor.layout(); }
      });
    } catch (error) { this.error.set(`The editor could not load: ${String(error)}`); }
  }
  private show(doc: EditorDocument | null, docs: EditorDocument[], theme: string, palette: string): void {
    if (!this.monaco || !this.editor) return;
    const monacoTheme =
      palette === 'contrast-dark'
        ? 'hc-black'
        : palette === 'contrast-light'
          ? 'hc-light'
          : theme === 'dark'
            ? 'vs-dark'
            : 'vs';
    if (this.appliedTheme !== monacoTheme) {
      this.monaco.editor.setTheme(monacoTheme);
      this.appliedTheme = monacoTheme;
    }
    this.updating = true;
    try {
      for (const [path, model] of this.models) if (!docs.some(item => item.path === path)) { model.dispose(); this.models.delete(path); }
      if (!doc) { this.editor.setModel(null); return; }
      let model = this.models.get(doc.path);
      if (!model) {
        const extension = doc.path.split('.').pop()?.toLowerCase() ?? '';
        const language = ({ ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript', rs: 'rust', py: 'python', html: 'html', css: 'css', json: 'json', md: 'markdown', sh: 'shell', yml: 'yaml', yaml: 'yaml' } as Record<string,string>)[extension] ?? 'plaintext';
        model = this.monaco.editor.createModel(doc.text, language);
        this.models.set(doc.path, model);
      } else if (model.getValue() !== doc.text) model.setValue(doc.text);
      if (this.editor.getModel() !== model) {
        const state = this.editor.saveViewState();
        if (state && this.shownPath) this.viewStates.set(this.shownPath, state);
        this.editor.setModel(model);
        this.shownPath = doc.path;
        const previous = this.viewStates.get(doc.path);
        if (previous) this.editor.restoreViewState(previous);
      }
      requestAnimationFrame(() => { if (!this.disposed) this.editor?.layout(); });
      this.editor.updateOptions({ readOnly: !!doc.readonly });
    } finally { this.updating = false; }
  }
  ngOnDestroy(): void { this.disposed = true; this.listeners.forEach(item => item.dispose()); this.editor?.dispose(); this.models.forEach(model => model.dispose()); }
}

@Component({
  selector: 'pa-files-card', imports: [MatButtonModule, FormsModule, Icon, CodeEditor, Diff, Action, ToolBar, ToolTab, SearchField, EmptyState],
  template: `
    <pa-tool-bar>
      <button matButton="tonal" class="btn btn-sm" (click)="begin('create_file')"><pa-icon name="file" [size]="16" />New file</button>
      <button matButton="tonal" class="btn btn-sm" (click)="begin('create_directory')"><pa-icon name="folder" [size]="16" />New folder</button>
      <span class="spacer"></span>
      @if (store.lastTrash()) { <pa-action icon="undo" label="Undo removal" (click)="restore()" /> }
      <pa-action icon="refresh" label="Refresh files" (click)="store.list()" />
    </pa-tool-bar>
    @if (operation()) {
      <form class="editor-path-form" (submit)="$event.preventDefault(); apply()">
        <label>{{ operation() === 'rename' ? 'New path' : 'Project path' }}<input class="input input-sm" name="path" [(ngModel)]="path" placeholder="src/example.ts" required /></label>
        <button matButton="filled" class="btn btn-sm btn-primary" type="submit">{{ operation() === 'rename' ? 'Move' : 'Create' }}</button>
        <button matButton class="btn btn-sm" type="button" (click)="operation.set('')">Cancel</button>
      </form>
    }
    @if (store.recovery.error()) { <p class="banner banner-danger tool-banner" role="alert">{{ store.recovery.error() }}</p> }
    @if (store.recoveryMessage()) { <p class="banner tool-banner" role="status">{{ store.recoveryMessage() }}</p> }
    @if (store.error()) { <p class="banner banner-danger tool-banner" role="alert"><pa-icon name="alert" [size]="16" />{{ store.error() }}</p> }
    <div class="editor-workspace">
      <nav class="editor-tree" aria-label="Project files">
        <div class="editor-explorer-head">
          <span class="editor-explorer-title">Explorer</span>
          <span class="editor-project-name truncate" [title]="workspaceName">{{ workspaceName }}</span>
        </div>
        <pa-search-field label="Filter files" placeholder="Filter files" [(value)]="filter" />
        <pa-search-field label="Search project text" placeholder="Search in files" [(value)]="query" (submitted)="search()" (cleared)="results.set(null)" />
        @if (results(); as found) {
          @for (file of found.files; track file.path) {
            <button class="editor-search-hit" (click)="store.open(file.path)"><span class="editor-hit-path">{{ file.path }}</span><span class="editor-hit-line">{{ file.lines?.[0]?.excerpt }}</span></button>
          } @empty { <p class="fine tree-note">No matching text.</p> }
        }
        @for (entry of store.entries(); track entry.path) {
          @if (!filter || entry.path.toLowerCase().includes(filter.toLowerCase())) {
            <div class="editor-tree-row" [class.active]="!entry.dir && store.activePath() === entry.path" [style.padding-left.px]="4 + (entry.depth || 0) * 14">
              <button class="editor-tree-open" (click)="store.toggle(entry)" [attr.title]="entry.path" [attr.aria-expanded]="entry.dir ? store.expanded().includes(entry.path) : null">
                <pa-icon class="editor-tree-chevron" [class.is-hidden]="!entry.dir" name="chevron-right" [size]="14" />
                <pa-icon class="editor-tree-icon" [name]="entry.dir ? 'folder' : 'file'" [size]="16" /><span>{{ entry.name }}</span>
              </button>
              <pa-action icon="pencil" [label]="'Rename ' + entry.name" (click)="begin('rename', entry.path)" />
              <pa-action icon="trash" tone="quiet-danger" [label]="'Remove ' + entry.name" (click)="remove(entry.path)" />
            </div>
          }
        } @empty { <p class="fine tree-note">{{ store.loading() ? 'Reading project…' : 'No files yet.' }}</p> }
      </nav>
      <section class="editor-main" aria-label="Editor">
        @if (store.documents().length) {
          <div class="tool-tabs" role="tablist" aria-label="Open files">
            @for (doc of store.documents(); track doc.path) {
              <pa-tool-tab animate.leave="tool-out" icon="file-code" [label]="doc.path.split('/').pop() || doc.path" [hint]="doc.path" [active]="store.activePath() === doc.path" [dirty]="store.dirty(doc)" (selected)="store.activePath.set(doc.path)" (closed)="store.close(doc)" />
            }
          </div>
        }
        @if (store.active(); as doc) {
          <pa-tool-bar class="editor-filebar">
            <span class="editor-path truncate" [title]="doc.path">{{ doc.path }}</span>
            <span class="spacer"></span>
            <pa-action icon="sparkles" label="Ask PWR about this file" (click)="store.ask()" />
            <pa-action icon="git-compare" [label]="compare() ? 'Back to editing' : 'Show changes'" [pressed]="compare()" (click)="compare.set(!compare())" />
            <pa-action icon="refresh" label="Reload from disk" (click)="store.reload()" />
            <button matButton="filled" class="btn btn-sm btn-primary" [disabled]="doc.readonly || !store.dirty(doc) || store.saving()" (click)="store.save()">Save</button>
          </pa-tool-bar>
          @if (doc.conflict) { <p class="banner banner-danger tool-banner" role="alert"><pa-icon name="alert" [size]="16" />This file changed on disk. Your unsaved text is preserved. Reload to use the disk version; Save checks the version before writing.</p> }
          @if (doc.readonly) { <p class="banner banner-warning tool-banner"><pa-icon name="info" [size]="16" />This file is shown partially and cannot be edited.</p> }
        } @else {
          <pa-empty-state icon="file-code" title="No file open">Pick a file in the explorer, or create a new one.</pa-empty-state>
        }
        @if (compare() && store.active(); as doc) { <div class="editor-diff card-scroll"><pa-diff [diff]="{path: doc.path, oldText: doc.saved, newText: doc.text}" /></div> }
        <pa-code-editor [style.display]="compare() || !store.active() ? 'none' : 'block'" />
        @if (store.active(); as doc) {
          <footer class="editor-status-bar">
            <span class="editor-status-state" [class.is-dirty]="store.dirty(doc)" [class.is-conflict]="doc.conflict">
              <pa-icon [name]="doc.conflict ? 'alert' : store.dirty(doc) ? 'circle-dot' : 'check-circle'" [size]="14" />
              {{ doc.conflict ? 'Conflict' : store.dirty(doc) ? 'Unsaved changes' : 'Saved' }}
            </span>
            <span class="spacer"></span><span>{{ language(doc.path) }}</span><span>Spaces: 2</span><span>UTF-8</span>
          </footer>
        }
      </section>
    </div>`, changeDetection: ChangeDetectionStrategy.OnPush,
})
export class FilesCard {
  protected readonly store = inject(EditorStore);
  private readonly confirm = inject(ConfirmService);
  private readonly agent = inject(AgentStore);
  protected readonly operation = signal('');
  protected path = ''; protected filter = ''; protected query = ''; private original = '';
  protected readonly results = signal<any>(null);
  protected readonly compare = signal(false);
  protected get workspaceName(): string {
    return this.agent.workspace().replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || 'Workspace';
  }
  protected language(path: string): string {
    const extension = path.split('.').pop()?.toLowerCase() ?? '';
    return ({ ts: 'TypeScript', tsx: 'TypeScript JSX', js: 'JavaScript', jsx: 'JavaScript JSX', rs: 'Rust', py: 'Python', html: 'HTML', css: 'CSS', json: 'JSON', md: 'Markdown', sh: 'Shell', yml: 'YAML', yaml: 'YAML' } as Record<string, string>)[extension] ?? 'Plain text';
  }
  protected async search(): Promise<void> {
    try { this.results.set(await this.agent.call('_pwr/editor', {cwd:this.agent.workspace(),operation:'search',query:this.query})); } catch (error) { this.store.error.set(String(error)); }
  }
  protected begin(operation: string, path = ''): void { this.operation.set(operation); this.original = path; this.path = path; }
  protected async apply(): Promise<void> {
    try {
      await this.store.mutate(this.operation(), this.operation() === 'rename' ? this.original : this.path, this.path);
      this.operation.set(''); this.store.error.set('');
    } catch (error) { this.store.error.set(String(error)); }
  }
  protected async remove(path: string): Promise<void> {
    if (!await this.confirm.ask({ title: 'Move to project trash?', subject: path, message: 'You can restore this item with Undo removal.', confirmLabel: 'Move to trash', tone: 'danger' })) return;
    try { await this.store.mutate('trash', path); } catch (error) { this.store.error.set(String(error)); }
  }
  protected async restore(): Promise<void> { try { await this.store.restore(); } catch (error) { this.store.error.set(String(error)); } }
}
