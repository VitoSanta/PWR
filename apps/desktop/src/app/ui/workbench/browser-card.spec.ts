import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { BrowserCard } from './cards';
import { AgentStore } from '../../core/agent.store';

describe('Preview address lifetime', () => {
  const workspace = signal('/a');
  beforeEach(() => {
    workspace.set('/a');
    TestBed.configureTestingModule({ imports: [BrowserCard], providers: [
      { provide: AgentStore, useValue: { workspace, timeline: signal([]) } },
    ] }).overrideComponent(BrowserCard, { set: { template: '', imports: [] } });
  });
  it('restores a manual local address when the collapsed card is recreated', () => {
    let fixture = TestBed.createComponent(BrowserCard);
    fixture.detectChanges();
    (fixture.componentInstance as any).go('localhost:5199/tickets');
    fixture.destroy();
    fixture = TestBed.createComponent(BrowserCard);
    fixture.detectChanges();
    expect((fixture.componentInstance as any).url()).toBe('http://localhost:5199/tickets');
    expect((fixture.componentInstance as any).frameAddress()).toBe('http://localhost:5199/tickets');
  });
  it('does not carry an address to another workspace and restores it on return', () => {
    const fixture = TestBed.createComponent(BrowserCard);
    fixture.detectChanges();
    (fixture.componentInstance as any).go('localhost:5199/tickets');
    workspace.set('/b'); fixture.detectChanges();
    expect((fixture.componentInstance as any).url()).toBe('');
    workspace.set('/a'); fixture.detectChanges();
    expect((fixture.componentInstance as any).url()).toBe('http://localhost:5199/tickets');
  });
});
