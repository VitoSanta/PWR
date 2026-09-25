import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { Memory, MemoryScope, Profile } from '../core/model';
import { PersonalStore } from '../core/personal.store';
import { Icon } from './kit/icon';
import { Tooltip } from './kit/tooltip';

type ProfileField = 'name' | 'role' | 'language' | 'style' | 'about';

/** Settings → Profile: who PWR is working with. */
@Component({
  selector: 'pa-profile-settings',
  imports: [Icon],
  template: `
    <header class="settings-page-head">
      <h3 class="settings-page-title">Profile</h3>
      <p class="fine">
        What PWR knows about you in every conversation. It reads this before each reply; nothing
        leaves this Mac.
      </p>
    </header>
    @if (personal.error()) {
      <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ personal.error() }}</p>
    }
    <div class="settings-form">
      @for (field of fields; track field.key) {
        <label class="settings-field" [class.settings-field-wide]="field.key === 'about'">
          <span class="settings-field-label">{{ field.label }}</span>
          @if (field.key === 'about') {
            <textarea
              class="input settings-textarea"
              rows="4"
              maxlength="600"
              [value]="draft()[field.key] ?? ''"
              (input)="set(field.key, $any($event.target).value)"
            ></textarea>
          } @else {
            <input
              class="input"
              maxlength="600"
              [value]="draft()[field.key] ?? ''"
              (input)="set(field.key, $any($event.target).value)"
            />
          }
          <span class="settings-field-hint">{{ field.hint }}</span>
        </label>
      }
    </div>
    <footer class="settings-page-foot">
      <span class="t-meta">{{ dirty() ? 'Unsaved changes' : saved() ? 'Saved' : '' }}</span>
      <button class="btn btn-primary" (click)="save()" [disabled]="personal.saving() || !dirty()">Save</button>
    </footer>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfileSettings {
  protected readonly personal = inject(PersonalStore);
  protected readonly draft = signal<Profile>({ memoryEnabled: true });
  protected readonly dirty = signal(false);
  protected readonly saved = signal(false);

  protected readonly fields: { key: ProfileField; label: string; hint: string }[] = [
    { key: 'name', label: 'Name', hint: 'How PWR addresses you.' },
    { key: 'role', label: 'Role', hint: 'What you do, so answers fit it.' },
    { key: 'language', label: 'Language', hint: 'The language PWR answers in.' },
    { key: 'style', label: 'Answer style', hint: 'Short or detailed, examples or not.' },
    { key: 'about', label: 'About you', hint: 'What you work on and anything PWR should keep in mind.' },
  ];

  constructor() {
    void this.personal.load().then(() => this.draft.set({ ...this.personal.profile() }));
  }

  protected set(key: ProfileField, value: string): void {
    this.draft.update((draft) => ({ ...draft, [key]: value }));
    this.dirty.set(true);
    this.saved.set(false);
  }

  protected async save(): Promise<void> {
    await this.personal.saveProfile({ ...this.draft(), memoryEnabled: this.personal.profile().memoryEnabled });
    if (!this.personal.error()) {
      this.draft.set({ ...this.personal.profile() });
      this.dirty.set(false);
      this.saved.set(true);
    }
  }
}

/** Settings → Memory: the facts PWR keeps between conversations. */
@Component({
  selector: 'pa-memory-settings',
  imports: [Icon, Tooltip],
  template: `
    <header class="settings-page-head">
      <h3 class="settings-page-title">Memory</h3>
      <p class="fine">
        Facts PWR keeps between conversations. A model can only propose one: it is saved when you
        confirm it, or when you add it here.
      </p>
    </header>
    @if (personal.error()) {
      <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ personal.error() }}</p>
    }
    <div class="settings-row">
      <div class="settings-row-text">
        <span class="settings-row-title">Use memory</span>
        <span class="fine">Off keeps what is saved and stops using it.</span>
      </div>
      <button
        type="button"
        class="switch-button"
        role="switch"
        [attr.aria-checked]="personal.profile().memoryEnabled"
        aria-label="Use memory"
        (click)="toggle()"
        [disabled]="personal.saving()"
      >
        <span class="switch" aria-hidden="true"></span>
      </button>
    </div>

    @for (group of groups(); track group.scope) {
      <section class="memory-card" [attr.aria-label]="group.label">
        <header class="memory-card-head">
          <span class="settings-row-title">{{ group.label }}</span>
          <span class="t-meta">{{ group.hint }}</span>
        </header>
        @if (list(group.scope).length) {
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
                    aria-label="Edit memory"
                  />
                } @else {
                  <span class="memory-text">{{ memory.text }}</span>
                  <button class="icon-btn icon-btn-sm" (click)="editing.set(memory.id)" aria-label="Edit" paTooltip="Edit">
                    <pa-icon name="pencil" [size]="14" />
                  </button>
                  <button class="icon-btn icon-btn-sm" (click)="personal.remove(group.scope, memory)" aria-label="Delete" paTooltip="Delete">
                    <pa-icon name="trash" [size]="14" />
                  </button>
                }
              </li>
            }
          </ul>
        } @else {
          <p class="memory-empty">Nothing saved yet.</p>
        }
        <form class="memory-add" (submit)="$event.preventDefault(); add(group.scope, input)">
          <input #input class="input input-sm" maxlength="600" placeholder="Add a fact…" [attr.aria-label]="'Add to ' + group.label" />
          <button class="btn btn-sm" type="submit" [disabled]="personal.saving()">Add</button>
        </form>
      </section>
    }

    @if (!agent.chatMode()) {
      <div class="settings-row">
        <div class="settings-row-text">
          <span class="settings-row-title">Project instructions</span>
          @if (personal.memories().instructions; as instructions) {
            <span class="fine">Read from <code>{{ instructions.path }}</code> in this workspace.</span>
          } @else {
            <span class="fine">Add <code>AGENTS.md</code> or <code>.pwr/instructions.md</code> to this workspace to give PWR its own instructions.</span>
          }
        </div>
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MemorySettings {
  protected readonly personal = inject(PersonalStore);
  protected readonly agent = inject(AgentStore);
  protected readonly editing = signal<string | null>(null);

  protected readonly groups = computed(() => {
    const all: { scope: MemoryScope; label: string; hint: string }[] = [
      { scope: 'global', label: 'About you', hint: 'Every conversation' },
    ];
    if (!this.agent.chatMode()) all.push({ scope: 'workspace', label: 'This workspace', hint: 'This project only' });
    return all;
  });

  constructor() {
    void this.personal.load();
  }

  protected toggle(): void {
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

/** Settings → Projects: the workspaces any conversation can recall by name. */
@Component({
  selector: 'pa-projects-settings',
  imports: [Icon, Tooltip],
  template: `
    <header class="settings-page-head">
      <h3 class="settings-page-title">Projects</h3>
      <p class="fine">
        Workspaces PWR keeps a wiki for. In any conversation you can ask about one by name. A workspace
        is added after PWR's first reply in it.
      </p>
    </header>
    @if (personal.error()) {
      <p class="banner banner-danger" role="alert"><pa-icon name="alert" [size]="16" />{{ personal.error() }}</p>
    }
    @if (personal.projects().length) {
      <ul class="project-list">
        @for (project of personal.projects(); track project.path) {
          <li class="project-item">
            <div class="project-text">
              <span class="settings-row-title">{{ project.name }}</span>
              @if (project.summary) {
                <span class="fine">{{ project.summary }}</span>
              }
              <span class="t-meta mono truncate">{{ project.path }}</span>
            </div>
            <button class="btn btn-sm" (click)="personal.forget(project)" paTooltip="Its folder and wiki stay on disk">Forget</button>
          </li>
        }
      </ul>
    } @else {
      <p class="memory-empty">No projects yet.</p>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProjectsSettings {
  protected readonly personal = inject(PersonalStore);

  constructor() {
    void this.personal.load();
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
          <span class="t-meta">Remember this?</span>
          <span>{{ proposal.text }}</span>
        </div>
        @if (!agent.chatMode()) {
          <button
            class="btn btn-sm"
            (click)="personal.accept(proposal, proposal.scope === 'global' ? 'workspace' : 'global')"
            [disabled]="personal.saving()"
            [paTooltip]="proposal.scope === 'global' ? 'Keep it for this workspace only' : 'Keep it for every conversation'"
          >
            {{ proposal.scope === 'global' ? 'Only here' : 'Everywhere' }}
          </button>
        }
        <button class="btn btn-sm btn-primary" (click)="personal.accept(proposal, agent.chatMode() ? 'global' : proposal.scope)" [disabled]="personal.saving()">
          {{ proposal.scope === 'global' || agent.chatMode() ? 'Remember' : 'Remember here' }}
        </button>
        <button class="icon-btn icon-btn-sm" (click)="personal.dismiss(proposal)" aria-label="Dismiss" paTooltip="Don't remember">
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
