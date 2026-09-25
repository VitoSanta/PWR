import { Directive, ElementRef, OnDestroy, OnInit, inject } from '@angular/core';

/**
 * Keeps a scrolling box on its newest lines while its content grows -- a
 * reasoning or a payload streaming in -- unless the person has scrolled up to
 * read, in which case it stays where they left it until they come back down.
 */
@Directive({
  selector: '[paFollow]',
  host: { '(scroll)': 'onScroll()' },
})
export class Follow implements OnInit, OnDestroy {
  private readonly element = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private pinned = true;
  private observer?: MutationObserver;

  ngOnInit(): void {
    this.stick();
    if (typeof MutationObserver === 'undefined') return;
    this.observer = new MutationObserver(() => {
      if (this.pinned) this.stick();
    });
    this.observer.observe(this.element, { childList: true, subtree: true, characterData: true });
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
  }

  protected onScroll(): void {
    const box = this.element;
    this.pinned = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
  }

  private stick(): void {
    this.element.scrollTop = this.element.scrollHeight;
  }
}
