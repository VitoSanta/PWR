import { ChangeDetectionStrategy, Component, effect, inject, untracked } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { FormsModule } from '@angular/forms';
import { BrowserStore, SEARCH_ENGINES } from '../../core/browser';
import { ToastService } from '../../core/ui';
import { bridge, inTauri } from '../../core/bridge';
import { WebView } from './web-view';
import { Icon } from '../kit/icon';
import { Action, EmptyState, ToolBar, ToolTab } from '../kit/tool';

/** No engine is favoured by its place in the list: the order changes each time it is shown. */
function shuffled<T>(items: readonly T[]): T[] {
  const copy = [...items];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}
@Component({
  selector: 'pa-browser-card', imports: [MatButtonModule, FormsModule, WebView, Icon, Action, EmptyState, ToolBar, ToolTab],
  template: `
    <div class="tool-tabs" role="tablist" aria-label="Browser tabs">
      @for (tab of store.tabs(); track tab.id) {
        <pa-tool-tab animate.leave="tool-out" icon="globe" [label]="tab.title" [hint]="tab.url" [active]="store.activeId() === tab.id" (selected)="store.select(tab.id)" (closed)="close(tab.id)" />
      }
      <pa-action icon="plus" label="New tab" (click)="store.openHome()" />
    </div>
    <form class="browser-address-bar" (submit)="$event.preventDefault(); go()">
      <pa-action icon="arrow-right" size="md" class="browser-back" label="Back" [disabled]="!store.active()" (click)="action('back')" />
      <pa-action icon="arrow-right" size="md" label="Forward" [disabled]="!store.active()" (click)="action('forward')" />
      <pa-action icon="refresh" size="md" label="Reload" [disabled]="!store.active()" (click)="action('reload')" />
      <input class="input input-sm browser-omnibox" aria-label="Browser address" name="address" [(ngModel)]="address" placeholder="Website address or localhost:3000" list="pwr-browser-history" />
      <datalist id="pwr-browser-history">@for (url of store.history().slice(0,20); track url) { <option [value]="url"></option> }</datalist>
      <button matButton="filled" class="btn btn-sm btn-primary" type="submit">Go</button>
      <pa-action icon="heart" size="md" label="Make this page your home page" [disabled]="!store.active()" (click)="makeHome()" />
      <pa-action icon="settings" size="md" label="Search engine" (click)="store.choosing.set(true)" />
      <pa-action icon="external-link" size="md" label="Open in your browser" [disabled]="!store.active()" (click)="external()" />
    </form>
    @if (store.error()) { <p class="banner banner-danger tool-banner" role="alert"><pa-icon name="alert" [size]="16" />{{ store.error() }}</p> }
    @if (store.choosing()) {
      <section class="engine-choice" aria-labelledby="engine-title">
        <header class="engine-head">
          <span class="empty-icon"><pa-icon name="search" [size]="24" /></span>
          <h3 id="engine-title" class="empty-title">Choose your search engine</h3>
          <p class="engine-text">It answers searches typed in the address bar and sets your home page. You can change it at any time.</p>
        </header>
        <div class="engine-list" role="radiogroup" aria-label="Search engine">
          @for (engine of engines; track engine.id) {
            <button class="engine-option" role="radio" [attr.aria-checked]="store.engine()?.id === engine.id" (click)="store.chooseEngine(engine)">
              <span class="engine-avatar" aria-hidden="true">{{ engine.name.charAt(0) }}</span>
              <span class="engine-label">
                <span class="engine-name">{{ engine.name }}</span>
                <span class="engine-host">{{ hostOf(engine.home) }}</span>
              </span>
              <span class="engine-radio" aria-hidden="true"></span>
            </button>
          }
        </div>
        @if (store.engine()) { <button matButton class="btn" (click)="store.choosing.set(false)">Keep {{ store.engine()?.name }}</button> }
      </section>
    } @else if (store.active(); as tab) {
      <pa-tool-bar class="browser-control">
        <span class="browser-controller" [class.is-model]="tab.controller === 'model'">{{ tab.controller === 'model' ? 'PWR controls this tab' : 'You control this tab' }}</span><span class="spacer"></span>
        @if (tab.controller === 'model') { <button matButton class="btn btn-sm" (click)="store.control('user')">Take control</button> }
        @else { <button matButton="tonal" class="btn btn-sm" (click)="store.control('model')"><pa-icon name="sparkles" [size]="16" />Give control to PWR</button> }
        <pa-action icon="eye" label="Inspect page" (click)="action('snapshot')" />
      </pa-tool-bar>
      @if (native) { <pa-web-view [tabId]="tab.id" [address]="tab.url" /> }
      @else { <pa-empty-state icon="globe" title="Desktop app only">Interactive browsing is available inside the PWR desktop app.</pa-empty-state> }
      @if (store.snapshot(); as snapshot) {
        <details class="browser-inspection" open><summary>Page text and {{ snapshot.elements?.length || 0 }} interactive elements</summary><pre>{{ snapshot.text }}</pre>
          <ul class="browser-elements">@for (element of snapshot.elements?.slice(0,50); track element.ref) {
            <li><span>{{ element.tag }} · {{ element.name || '(unnamed element)' }}</span>
              <button matButton class="btn btn-sm" [disabled]="element.disabled" (click)="clickElement(element.ref)">Click</button></li>
          }</ul>
        </details>
      }
    } @else { <pa-empty-state icon="globe" title="Open a page">Open a website or a development server. PWR can use the same tab after you share control.<br /><button matButton="tonal" class="btn empty-action" (click)="store.openHome()">Open {{ homeName() }}</button></pa-empty-state> }
  `, changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BrowserCard {
  protected readonly store = inject(BrowserStore);
  protected readonly native = inTauri();
  private readonly toast = inject(ToastService);
  protected address = '';
  protected readonly engines = shuffled(SEARCH_ENGINES);
  protected hostOf(url: string): string { return new URL(url).hostname.replace(/^www\./, ''); }
  constructor() {
    effect(() => { const tab = this.store.active(); untracked(() => { this.address = tab?.url ?? ''; }); });
    // The first time the Browser opens it shows the home page, unless PWR is the one opening a tab.
    if (this.native) setTimeout(() => { if (!this.store.tabs().length && !this.store.agentBusy) void this.store.openHome(); });
  }
  protected homeName(): string { return new URL(this.store.home()).hostname.replace(/^www\./, ''); }
  protected makeHome(): void {
    const url = this.store.active()?.url;
    if (!url) return;
    try { this.store.setHome(url); this.toast.show('Home page set', 'success', 1800); } catch (error) { this.store.error.set(String(error)); }
  }
  protected async go(): Promise<void> {
    try {
      const address = this.store.resolve(this.address);
      if (!this.store.active()) await this.store.open(address);
      else await this.store.navigate(address);
    } catch (error) { this.store.error.set(String(error).replace(/^Error: /, '')); }
  }
  protected action(operation: string): void { void this.store.action(operation).catch(() => {}); }
  protected clickElement(ref: string): void { void this.store.action('click', {ref}).then(() => this.store.action('snapshot')).catch(() => {}); }
  protected close(id: string): void { void this.store.close(id).catch(error => this.store.error.set(String(error))); }
  protected external(): void { const url = this.store.active()?.url; if (url) void bridge.openExternal(url); }
}
