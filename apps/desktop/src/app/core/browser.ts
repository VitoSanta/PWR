import { DestroyRef, Injectable, computed, effect, inject, signal, untracked } from '@angular/core';
import { AgentStore } from './agent.store';
import { WorkbenchStore } from './workbench';
import { bridge, inTauri } from './bridge';

export interface BrowserTab { id: string; url: string; title: string; controller: 'user' | 'model'; conversationId?: string | null }
export function browserAddress(raw: string): string {
  const text = raw.trim();
  const url = new URL(/^[a-z][a-z\d+.-]*:\/\//i.test(text) ? text : `${/^(localhost|127\.0\.0\.1)(:|\/|$)/i.test(text) ? 'http' : 'https'}://${text}`);
  if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password || ['tauri.localhost','ipc.localhost'].includes(url.hostname)) throw new Error('Enter an HTTP or HTTPS website address.');
  if (url.origin === globalThis.location?.origin) throw new Error('The PWR interface cannot open inside the browser.');
  return url.href;
}
const HOME_KEY = 'pwr:browser-home';
const ENGINE_KEY = 'pwr:browser-engine';

export interface SearchEngine { id: string; name: string; home: string; search: string }
/** What a person may choose as the Browser's search engine; none is preselected. */
export const SEARCH_ENGINES: SearchEngine[] = [
  { id: 'google', name: 'Google', home: 'https://www.google.com/', search: 'https://www.google.com/search?q=%s' },
  { id: 'bing', name: 'Bing', home: 'https://www.bing.com/', search: 'https://www.bing.com/search?q=%s' },
  { id: 'duckduckgo', name: 'DuckDuckGo', home: 'https://duckduckgo.com/', search: 'https://duckduckgo.com/?q=%s' },
  { id: 'brave', name: 'Brave Search', home: 'https://search.brave.com/', search: 'https://search.brave.com/search?q=%s' },
  { id: 'ecosia', name: 'Ecosia', home: 'https://www.ecosia.org/', search: 'https://www.ecosia.org/search?q=%s' },
  { id: 'startpage', name: 'Startpage', home: 'https://www.startpage.com/', search: 'https://www.startpage.com/do/search?q=%s' },
  { id: 'qwant', name: 'Qwant', home: 'https://www.qwant.com/', search: 'https://www.qwant.com/?q=%s' },
];
function loadEngine(): SearchEngine | null {
  try { return SEARCH_ENGINES.find(engine => engine.id === localStorage.getItem(ENGINE_KEY)) ?? null; } catch { return null; }
}
function loadHome(engine: SearchEngine | null): string {
  const fallback = engine?.home ?? SEARCH_ENGINES[0].home;
  try { return browserAddress(localStorage.getItem(HOME_KEY) ?? fallback); } catch { return fallback; }
}
/** Whether typed text is an address (a scheme, a host with a dot, localhost, an IP) rather than words to search for. */
function looksLikeAddress(text: string): boolean {
  return /^[a-z][a-z\d+.-]*:\/\//i.test(text) || /^(localhost|\d{1,3}(\.\d{1,3}){3})(:\d+)?([/?#]|$)/i.test(text)
    || /^[^\s/?#]+\.[a-z]{2,}(:\d+)?([/?#]\S*)?$/i.test(text);
}
@Injectable({ providedIn: 'root' })
export class BrowserStore {
  private readonly agent = inject(AgentStore);
  private readonly work = inject(WorkbenchStore);
  readonly tabs = signal<BrowserTab[]>([]);
  readonly activeId = signal('');
  readonly active = computed(() => this.tabs().find(tab => tab.id === this.activeId()) ?? null);
  readonly error = signal('');
  readonly busy = signal(false);
  readonly snapshot = signal<any>(null);
  readonly history = signal<string[]>([]);
  /** The person's search engine; until they choose one, the Browser asks. */
  readonly engine = signal<SearchEngine | null>(loadEngine());
  readonly choosing = signal(this.engine() === null);
  /** Where a new tab, and the Browser when it is first opened, go. */
  readonly home = signal(loadHome(this.engine()));
  /** Counts the agent's own requests, so the Browser does not add a home tab beside the one it opens. */
  agentBusy = 0;
  private cwd = '';
  /** Who waits for a tab's page to finish loading. */
  private readonly loadWaiters = new Map<string, () => void>();
  private sequence: Promise<unknown> = Promise.resolve();
  constructor() {
    const destroyRef = inject(DestroyRef);
    const stopControl = () => this.tabs.update(tabs => tabs.map(tab => ({ ...tab, controller: 'user' })));
    this.agent.workspaceStops.add(stopControl);
    destroyRef.onDestroy(() => {
      this.agent.workspaceStops.delete(stopControl);
      this.loadWaiters.forEach(done => done());
      this.loadWaiters.clear();
    });
    if (inTauri()) {
      const unlisten: Array<() => void> = [];
      void bridge.onBrowserNavigation(event => this.tabs.update(tabs => tabs.map(tab => tab.id === event.id ? { ...tab, url: event.url } : tab))).then(stop => unlisten.push(stop));
      void bridge.onBrowserTitle(event => this.tabs.update(tabs => tabs.map(tab => tab.id === event.id ? { ...tab, title: event.title } : tab))).then(stop => unlisten.push(stop));
      void bridge.onBrowserLoaded(event => { if (event.finished) this.loadWaiters.get(event.id)?.(); }).then(stop => unlisten.push(stop));
      destroyRef.onDestroy(() => unlisten.forEach(stop => stop()));
    }
    effect(() => {
      const engine = this.engine();
      this.agent.searchEngine.set(engine ? { name: engine.name, search: engine.search } : null);
    });
    effect(() => {
      const cwd = this.agent.workspace();
      untracked(() => {
        if (cwd === this.cwd) return;
        const previous = this.tabs(); this.tabs.set([]); this.activeId.set(''); this.snapshot.set(null);
        previous.forEach(tab => {
          this.loadWaiters.get(tab.id)?.();
          void bridge.browserClose(tab.id).catch(() => {});
        });
        this.cwd = cwd;
        try { this.history.set(JSON.parse(localStorage.getItem(`pwr:browser-history:${cwd}`) ?? '[]')); } catch { this.history.set([]); }
      });
    });
    this.agent.workspaceHandlers.set('browser', (operation, args) => {
      this.agentBusy++;
      const requestedSession = this.agent.sessionId(), requestedWorkspace = this.agent.workspace(), epoch = this.agent.workspaceEpoch();
      const run = this.sequence.then(async () => {
        if (epoch !== this.agent.workspaceEpoch() || requestedSession !== this.agent.sessionId() || requestedWorkspace !== this.agent.workspace() || !this.agent.turnActive()) throw new Error('The requesting turn ended or the workspace changed.');
        this.work.show('browser');
        if (operation === 'tabs') return { tabs: this.tabs() };
        if (operation === 'open') { const tab = await this.open(String(args.url ?? ''), 'model', true); return { tab: this.tabs().find(item => item.id === tab.id) ?? tab }; }
        const mine = this.tabs().filter(item => item.controller === 'model' && (!item.conversationId || item.conversationId === this.agent.sessionId()));
        const tab = args.tabId ? this.tabs().find(item => item.id === String(args.tabId)) : (this.active()?.controller === 'model' && (!this.active()?.conversationId || this.active()?.conversationId === this.agent.sessionId()) ? this.active()! : mine.at(-1));
        if (!tab) throw new Error('Choose a tabId from browser tabs, or open a page first with operation "open".');
        if (tab.controller !== 'model' || (tab.conversationId && tab.conversationId !== this.agent.sessionId())) throw new Error('This tab is controlled by the engineer. Ask them to give control to PWR.');
        // Control given before the conversation had an id belongs to the one that is asking.
        if (!tab.conversationId) this.tabs.update(tabs => tabs.map(item => item.id === tab.id ? { ...item, conversationId: this.agent.sessionId() } : item));
        this.activeId.set(tab.id);
        // Allow Angular and the native surface to reveal the selected tab.
        await new Promise(resolve => setTimeout(resolve, 150));
        const requireControl = () => {
          const current = this.tabs().find(item => item.id === tab.id);
          if (epoch !== this.agent.workspaceEpoch() || requestedSession !== this.agent.sessionId()
              || requestedWorkspace !== this.agent.workspace() || !this.agent.turnActive()
              || this.activeId() !== tab.id || current?.controller !== 'model'
              || current.conversationId !== requestedSession) throw new Error('Browser control changed.');
        };
        requireControl();
        if (operation === 'close') { await this.close(tab.id); return { closed: true }; }
        // The page's view appears a moment after the tab is chosen: ask again rather than fail on the first look.
        for (let attempt = 0; ; attempt++) {
          requireControl();
          try { return await this.action(operation, args, attempt < 4, tab.id); }
          catch (error) {
            if (attempt >= 4 || !/must be visible/.test(String(error))) { this.error.set(String(error)); throw error; }
            await new Promise(resolve => setTimeout(resolve, 300));
          }
        }
      });
      this.sequence = run.catch(() => {});
      void run.then(() => this.agentBusy--, () => this.agentBusy--);
      return run;
    });
  }
  async open(raw: string, controller: 'user' | 'model' = 'user', waitForLoad = false): Promise<BrowserTab> {
    const url = browserAddress(raw), cwd = this.cwd;
    // An explicit address does not require choosing a search provider.
    this.choosing.set(false);
    if (this.tabs().length >= 12) throw new Error('Close a tab before opening another.');
    const tab: BrowserTab = { id: crypto.randomUUID(), url, title: new URL(url).hostname, controller,
      conversationId: controller === 'model' ? this.agent.sessionId() : null };
    const loaded = waitForLoad ? this.loadOf(tab.id) : null;
    await bridge.browserOpen(tab.id, url);
    if (cwd !== this.cwd) { await bridge.browserClose(tab.id); throw new Error('The workspace changed.'); }
    this.tabs.update(tabs => [...tabs, tab]); this.activeId.set(tab.id); this.remember(url); this.error.set('');
    // What the model gets back is the page it asked for, not a blank one still loading.
    if (loaded) await loaded;
    return tab;
  }
  /** Resolves when a tab's page has loaded, or after ten seconds; at once outside the app. */
  private loadOf(id: string): Promise<void> {
    if (!inTauri()) return Promise.resolve();
    return new Promise<void>(resolve => {
      const done = () => { clearTimeout(timer); this.loadWaiters.delete(id); resolve(); };
      const timer = setTimeout(done, 10000);
      this.loadWaiters.set(id, done);
    });
  }
  async navigate(raw: string): Promise<void> {
    const tab = this.active(); if (!tab) { await this.open(raw); return; }
    const url = browserAddress(raw), cwd = this.cwd; await bridge.browserOpen(tab.id, url);
    if (cwd !== this.cwd) { await bridge.browserClose(tab.id); throw new Error('The workspace changed.'); }
    this.choosing.set(false);
    this.tabs.update(tabs => tabs.map(item => item.id === tab.id ? { ...item, url } : item)); this.remember(url);
  }
  /** A link the person followed: its page in the Browser, the tab it already has if it has one. Throws for an address the Browser refuses. */
  async openLink(raw: string): Promise<void> {
    const url = browserAddress(raw);
    this.choosing.set(false);
    const existing = this.tabs().find(tab => tab.url === url);
    if (existing) { this.select(existing.id); return; }
    await this.open(url);
  }
  async openHome(): Promise<void> {
    if (this.choosing()) return;
    try { await this.open(this.home()); } catch (error) { this.error.set(String(error).replace(/^Error: /, '')); }
  }
  /** An address as typed: a page, or words to search for with the chosen engine. */
  resolve(raw: string): string {
    const text = raw.trim();
    const engine = this.engine();
    if (!text || looksLikeAddress(text) || !engine) return text;
    return engine.search.replace('%s', encodeURIComponent(text));
  }
  /** The choice also sets the home page, and opens it when nothing is open. */
  chooseEngine(engine: SearchEngine): void {
    this.engine.set(engine);
    this.home.set(engine.home);
    try { localStorage.setItem(ENGINE_KEY, engine.id); localStorage.removeItem(HOME_KEY); } catch { /* The choice holds until the app closes. */ }
    this.choosing.set(false);
    if (!this.tabs().length) void this.openHome();
  }
  setHome(raw: string): void {
    const url = browserAddress(raw);
    this.home.set(url);
    try { localStorage.setItem(HOME_KEY, url); } catch { /* The choice holds until the app closes. */ }
  }
  select(id: string): void { this.activeId.set(id); this.snapshot.set(null); }
  control(controller: 'user' | 'model'): void {
    const id = this.activeId(); this.tabs.update(tabs => tabs.map(tab => tab.id === id ? { ...tab, controller, conversationId: controller === 'model' ? this.agent.sessionId() : null } : tab));
  }
  async action(operation: string, args: any = {}, quiet = false, tabId = this.activeId()): Promise<any> {
    const tab = this.tabs().find(item => item.id === tabId); if (!tab) return;
    this.busy.set(true); this.error.set('');
    try {
      const result = await bridge.browserAction(tab.id, operation, args);
      if (operation === 'snapshot') {
        this.snapshot.set(result);
        this.tabs.update(tabs => tabs.map(item => item.id === tab.id ? { ...item, title: result.title || item.title, url: result.url || item.url } : item));
      }
      return result;
    } catch (error) { if (!quiet) this.error.set(String(error)); throw error; }
    finally { this.busy.set(false); }
  }
  async close(id: string): Promise<void> {
    this.loadWaiters.get(id)?.();
    await bridge.browserClose(id); const tabs = this.tabs().filter(tab => tab.id !== id); this.tabs.set(tabs);
    if (this.activeId() === id) this.activeId.set(tabs.at(-1)?.id ?? '');
    this.snapshot.set(null);
  }
  private remember(url: string): void {
    this.history.update(all => [url, ...all.filter(item => item !== url)].slice(0, 100));
    try { localStorage.setItem(`pwr:browser-history:${this.cwd}`, JSON.stringify(this.history())); } catch { /* Session history remains. */ }
  }
}
