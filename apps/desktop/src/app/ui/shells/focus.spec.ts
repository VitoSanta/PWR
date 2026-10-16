import { vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { FocusShell } from './focus';
import { UiStore } from '../../core/ui';

describe('FocusShell run controls', () => {
  it('opens the run controls from Goal & approvals in the options menu', async () => {
    TestBed.configureTestingModule({ imports: [FocusShell] });
    const fixture = TestBed.createComponent(FocusShell);
    const ui = TestBed.inject(UiStore);
    const scrollTo = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollTo');
    Object.defineProperty(HTMLElement.prototype, 'scrollTo', { configurable: true, value: vi.fn() });
    fixture.detectChanges();
    const toolsButton = fixture.nativeElement.querySelector('.composer-tools-button') as HTMLButtonElement;

    toolsButton.click();
    fixture.detectChanges();

    const goalRow = [...fixture.nativeElement.querySelectorAll('.tool-strip-item') as NodeListOf<HTMLButtonElement>]
      .find((button) => button.textContent?.includes('Goal & approvals'));
    expect(goalRow).toBeDefined();
    goalRow!.click();
    fixture.detectChanges();

    expect(ui.runControls()).toBe(toolsButton);
    expect(fixture.nativeElement.querySelector('pa-run-controls')).not.toBeNull();
    fixture.destroy();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    if (scrollTo) Object.defineProperty(HTMLElement.prototype, 'scrollTo', scrollTo);
    else Reflect.deleteProperty(HTMLElement.prototype, 'scrollTo');
  });
});