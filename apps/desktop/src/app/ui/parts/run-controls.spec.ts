import { vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { AgentStore } from '../../core/agent.store';
import { RunControls } from './run-controls';

describe('Run controls permission modes', () => {
  it('switches Full access, sandboxed Auto and Ask through the core', async () => {
    TestBed.configureTestingModule({ imports: [RunControls] });
    const store = TestBed.inject(AgentStore);
    let mode = 'ask';
    const requests: string[] = [];
    store.useDemo((method, params) => {
      expect(method).toBe('_pwr/approvals');
      mode = params.mode ?? mode;
      requests.push(mode);
      return { mode, sandboxed: mode !== 'full', asking: [] };
    });
    const fixture = TestBed.createComponent(RunControls);
    fixture.componentRef.setInput('anchor', document.createElement('button'));
    fixture.detectChanges();
    const option = (label: string): HTMLButtonElement | undefined =>
      [...fixture.nativeElement.querySelectorAll('.focus-run-option') as NodeListOf<HTMLButtonElement>]
        .find(button => button.querySelector('strong')?.textContent === label);
    const full = option('Full access');
    expect(full).toBeDefined();
    full!.click();
    await vi.waitFor(() => expect(store.permissionMode()).toBe('full'));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(store.permissionMode()).toBe('full');
    expect(store.sandboxed()).toBe(false);
    expect(full!.getAttribute('aria-pressed')).toBe('true');
    const auto = option('Auto-approve')!;
    auto.click();
    await vi.waitFor(() => expect(store.permissionMode()).toBe('auto'));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(store.permissionMode()).toBe('auto');
    expect(store.sandboxed()).toBe(true);
    expect(full!.getAttribute('aria-pressed')).toBe('false');
    auto.click();
    await vi.waitFor(() => expect(store.permissionMode()).toBe('ask'));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(store.permissionMode()).toBe('ask');
    expect(store.sandboxed()).toBe(true);
    full!.click();
    await vi.waitFor(() => expect(store.permissionMode()).toBe('full'));
    fixture.detectChanges();
    full!.click();
    await vi.waitFor(() => expect(store.permissionMode()).toBe('ask'));
    expect(store.sandboxed()).toBe(true);
    expect(requests).toEqual(['full', 'auto', 'ask', 'full', 'ask']);
    fixture.destroy();
  });
});
