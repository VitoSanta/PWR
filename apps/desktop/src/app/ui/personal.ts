import { ChangeDetectionStrategy, Component, OnInit, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { Memory, MemoryScope, Profile } from '../core/model';
import { PersonalStore } from '../core/personal.store';
import { Icon } from './kit/icon';
import { Tooltip } from './kit/tooltip';

type ProfileField = 'name' | 'role' | 'language' | 'style' | 'about';

/**
 * Settings → You: who PWR is working with, and what it remembers. Everything
 * here is added to the model's instructions on the next reply, and counted in
 * the Context panel as "Profile and memory".
 */
@Component({
  selector: 'pa-personal-settings',
  imports: [Icon, Tooltip],
  template: `
    <section class="settings-group" aria-labelledby="profile-label">
      <h3 class="section-label" id="profile-label">You</h3>
      <div class="personal-fields">
        @for (field of fields; track field.key) {
          <label class="personal-field" [class.personal-field-wide]="field.key === 'about'">
            <span class="field-label">{{ field.label }}</span>
            @if (field.key === 'about') {
              <textarea
                class="input personal-textarea"
                rows="3"
                maxlength="600"
                [placeholder]="field.placeholder"
                [value]="draft()[field.key] ?? ''"
                (input)="set(field.key, $any($event.target).value)"
              ></textarea>
            } @else {
              <input
                class="input"
                maxlength="600"
                [placeholder]="field.placeholder"
                [value]="draft()[field.key] ?? ''"
                (input)="set(field.key, $any($event.target).value)"
              />
            }
          </label>
        }
      </div>
      <div class="personal-actions">
        <p class="fine">Added to what the model reads, from the next reply. Kept in ~/.pwr on this Mac.</p>
        <button class="btn btn-sm btn-primary" (click)="saveProfile()" [disabled]="personal.saving() || !dirty()">
          Save
        </button>
      </div>
    </section>

    <section class="settings-group" aria-labelledby="memory-label">
      <h3 class="section-label" id="memory-label">
        Memory
        <button
          type="button"
          class="toggle-chip"
          [attr.aria-pressed]="personal.profile().memoryEnabled"
          (click)="toggleMemory()"
          [disabled]="personal.saving()"
          paTooltip="Off keeps what is saved, and stops using it"
        >
          <span class="switch" aria-hidden="true"></span> Use memory
        </button>
      </h3>
      <p class="fine">
        Facts PWR keeps between conversations. The model can only propose one; it is saved when you
        confirm it, or when you add it here.
      </p>
      @if (personal.error()) {
        <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ personal.error() }}</p>
      }
      @for (group of groups; track group.scope) {
        <div class="memory-group">
          <div class="memory-group-head">
            <span class="field-label">{{ group.label }}</span>
            <span class="t-meta">{{ group.hint }}</span>
          </div>
          <ul class="memory-list">
            @for (memory of list(group.scope); track memory.id) {
              <li class="memory-item">
                @if (editing() === memory.id) {
                  <input
                    class="input input-sm"
                    maxlength="600"
                    [value]="memory.text"
                    (keydown.enter)="commit(group.scope, memory, $any($event.target).value)"
                    (keydown.escape)="editing.set(null)"
                    (blur)="commit(group.scope, memory, $any($event.target).value)"
                    [attr.aria-label]="'Edit memory'"
                    data-autofocus
                  />
                } @else {
                  <span class="memory-text">{{ memory.text }}</span>
                  <span class="memory-actions">
                    <button class="icon-btn icon-btn-sm" (click)="editing.set(memory.id)" aria-label="Edit" paTooltip="Edit">
                      <pa-icon name="pencil" [size]="14" />
                    </button>
                    <button class="icon-btn icon-btn-sm" (click)="personal.remove(group.scope, memory)" aria-label="Delete" paTooltip="Delete">
                      <pa-icon name="trash" [size]="14" />
                    </button>
                  </span>
                }
              </li>
            } @empty {
              <li class="memory-empty t-meta">Nothing yet.</li>
            }
          </ul>
          <form class="memory-add" (submit)="$event.preventDefault(); add(group.scope, input)">
            <input #input class="input input-sm" maxlength="600" [placeholder]="group.placeholder" [attr.aria-label]="'Add to ' + group.label" />
            <button class="btn btn-sm" type="submit" [disabled]="personal.saving()">Add</button>
          </form>
        </div>
      }
      <div class="memory-group">
        <div class="memory-group-head">
          <span class="field-label">Projects</span>
          <span class="t-meta">recalled by name from any conversation</span>
        </div>
        <ul class="memory-list">
          @for (project of personal.projects(); track project.path) {
            <li class="memory-item">
              <span class="memory-text">
                <strong>{{ project.name }}</strong>
                @if (project.summary) { · {{ project.summary }} }
                <span class="t-meta project-path">{{ project.path }}</span>
              </span>
              <span class="memory-actions">
                <button class="icon-btn icon-btn-sm" (click)="personal.forget(project)" aria-label="Forget" paTooltip="Forget this project (its folder and wiki stay)">
                  <pa-icon name="x" [size]="14" />
                </button>
              </span>
            </li>
          } @empty {
            <li class="memory-empty t-meta">A workspace is added after PWR's first reply in it.</li>
          }
        </ul>
      </div>
      @if (personal.memories().instructions; as instructions) {
        <p class="fine">
          This workspace's instructions are read from <code>{{ instructions.path }}</code>
          ({{ instructions.chars }} characters).
        </p>
      } @else if (!agent.chatMode()) {
        <p class="fine">
          Add <code>AGENTS.md</code> or <code>.pwr/instructions.md</code> to the workspace to give PWR the
          project's own instructions.
        </p>
      }
    </section>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PersonalSettings implements OnInit {
  protected readonly personal = inject(PersonalStore);
  protected readonly agent = inject(AgentStore);
  protected readonly draft = signal<Profile>({ memoryEnabled: true });
  protected readonly dirty = signal(false);
  protected readonly editing = signal<string | null>(null);

  protected readonly fields: { key: ProfileField; label: string; placeholder: string }[] = [
    { key: 'name', label: 'Name', placeholder: 'Vito' },
    { key: 'role', label: 'Role', placeholder: 'Software engineer' },
    { key: 'language', label: 'Answer in', placeholder: 'Italiano' },
    { key: 'style', label: 'Style', placeholder: 'Concise, with code examples' },
    { key: 'about', label: 'About you', placeholder: 'What you work on, what you know well, anything PWR should keep in mind' },
  ];

  protected readonly groups: { scope: MemoryScope; label: string; hint: string; placeholder: string }[] = [
    { scope: 'global', label: 'About you', hint: 'every conversation', placeholder: 'I prefer Angular signals to RxJS' },
    { scope: 'workspace', label: 'This workspace', hint: 'this project only', placeholder: 'We use pnpm, not npm' },
  ];

  async ngOnInit(): Promise<void> {
    await this.personal.load();
    this.draft.set({ ...this.personal.profile() });
    this.dirty.set(false);
  }

  protected set(key: ProfileField, value: string): void {
    this.draft.update((draft) => ({ ...draft, [key]: value }));
    this.dirty.set(true);
  }

  protected async saveProfile(): Promise<void> {
    await this.personal.saveProfile({ ...this.draft(), memoryEnabled: this.personal.profile().memoryEnabled });
    if (!this.personal.error()) {
      this.draft.set({ ...this.personal.profile() });
      this.dirty.set(false);
    }
  }

  protected toggleMemory(): void {
    const profile = this.personal.profile();
    void this.personal.saveProfile({ ...profile, memoryEnabled: !profile.memoryEnabled });
  }

  protected list(scope: MemoryScope): Memory[] {
    // Newest first, as the model reads them.
    return [...this.personal.memories()[scope]].reverse();
  }

  protected async add(scope: MemoryScope, input: HTMLInputElement): Promise<void> {
    const text = input.value.trim();
    if (!text) return;
    await this.personal.add(scope, text);
    if (!this.personal.error()) input.value = '';
  }

  protected async commit(scope: MemoryScope, memory: Memory, text: string): Promise<void> {
    if (this.editing() !== memory.id) return;
    this.editing.set(null);
    if (text.trim() !== memory.text) await this.personal.update(scope, memory, text);
  }
}

/**
 * What the model proposed to remember, above the composer: saved only when
 * the person says so, in the scope they choose.
 */
@Component({
  selector: 'pa-memory-proposals',
  imports: [Icon, Tooltip],
  template: `
    @for (proposal of personal.proposals(); track proposal.key) {
      <div class="memory-proposal" role="status">
        <pa-icon name="sparkles" [size]="16" />
        <div class="memory-proposal-text">
          <span class="t-meta">PWR would like to remember</span>
          <span>{{ proposal.text }}</span>
        </div>
        <button
          class="btn btn-sm"
          (click)="personal.accept(proposal, proposal.scope === 'global' ? 'workspace' : 'global')"
          [disabled]="personal.saving() || (agent.chatMode() && proposal.scope === 'global')"
          [paTooltip]="proposal.scope === 'global' ? 'Keep it for this workspace only' : 'Keep it for every conversation'"
        >
          {{ proposal.scope === 'global' ? 'This workspace' : 'Everywhere' }}
        </button>
        <button class="btn btn-sm btn-primary" (click)="personal.accept(proposal)" [disabled]="personal.saving()">
          Save{{ proposal.scope === 'global' ? '' : ' here' }}
        </button>
        <button class="icon-btn icon-btn-sm" (click)="personal.dismiss(proposal)" aria-label="Dismiss" paTooltip="Don't save">
          <pa-icon name="x" [size]="14" />
        </button>
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MemoryProposals {
  protected readonly personal = inject(PersonalStore);
  protected readonly agent = inject(AgentStore);
}
