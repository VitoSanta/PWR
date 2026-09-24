import { Injectable, computed, signal } from '@angular/core';
import { bridge, inTauri } from './bridge';
import { playDemo } from './demo';
import {
  CONTEXT_STEPS,
  ContextInfo,
  ContextWindow,
  ModelCompatibility,
  ReasoningEffort,
  ReasoningInfo,
  Entry,
  FileDiff,
  PermissionRequest,
  SessionSummary,
  modelLabel,
} from './model';

type Pending = { resolve: (value: any) => void; reject: (error: Error) => void };

/**
 * The app's whole state, as signals, fed by `pwr serve --stdio`.
 *
 * One stream of protocol messages drives everything: a response settles the
 * request that asked for it, a `session/update` changes the timeline, and a
 * request from the core (a permission) waits for the person.
 */
@Injectable({ providedIn: 'root' })
export class AgentStore {
  // Connection and workspace.
  readonly workspace = signal('');
  /** Chat mode's folder, from the core: a conversation there has no workspace. */
  readonly chatHome = signal('');
  /** Talking without a workspace: the model only reads what is attached. */
  readonly chatMode = computed(() => this.chatHome() !== '' && this.workspace() === this.chatHome());
  /** The workspace to go back to from chat mode. */
  private lastWorkspace = '';
  readonly corePath = signal('');
  readonly coreState = signal<'starting' | 'ready' | 'stopped' | 'error'>('starting');
  readonly coreError = signal('');
  readonly logs = signal<string[]>([]);

  // Models and context.
  readonly models = signal<string[]>([]);
  /** Models whose configuration declares a vision encoder (read by the core). */
  readonly visionModels = signal<string[]>([]);
  /** Whether the selected model could see an image. */
  readonly modelSees = computed(() => {
    const model = this.model();
    return model !== null && this.visionModels().includes(model);
  });
  readonly model = signal<string | null>(null);
  readonly reasoningEffort = signal<ReasoningEffort>('medium');
  /** How Reasoning Effort applies to the selected model, as the core decided. */
  readonly reasoning = signal<ReasoningInfo | null>(null);
  readonly compatibility = signal<ModelCompatibility | null>(null);
  readonly calibrating = signal(false);
  readonly calibrationProgress = signal<{ step: number; total: number; name: string } | null>(
    null,
  );
  readonly calibrationError = signal('');
  private calibrationId: string | null = null;
  readonly backendNote = signal('');
  readonly context = signal<ContextWindow | null>(null);

  // Sessions and the conversation.
  readonly sessions = signal<SessionSummary[]>([]);
  readonly sessionId = signal<string | null>(null);
  readonly timeline = signal<Entry[]>([]);
  readonly turnActive = signal(false);
  readonly lastEventAt = signal(Date.now());
  readonly outcome = signal('');
  readonly changes = signal<FileDiff[]>([]);
  readonly commandOutput = signal<{ name: string; text: string } | null>(null);
  readonly permission = signal<PermissionRequest | null>(null);
  /** Tokens the conversation occupies and the window, after the last reply. */
  readonly usage = signal<{ used: number; window: number } | null>(null);
  /** The context panel's account, read when it is opened or after a change. */
  readonly contextInfo = signal<ContextInfo | null>(null);
  readonly compacting = signal(false);

  // The composer.
  readonly attachments = signal<string[]>([]);
  // Off by default: a question should be answered, not turned into a task the
  // model must keep acting on (a "how do I start the game?" made it run the
  // server twice until the timeout, 2026-09-22).
  readonly goalMode = signal(false);
  /** Ask before what leaves the workspace, or run with every permission. */
  readonly permissionMode = signal<'ask' | 'auto'>('ask');
  /** What the core actually asks about in the current mode. */
  readonly asking = signal<string[]>([]);
  /** False where the platform gives no sandbox: commands then run unconfined. */
  readonly sandboxed = signal(true);
  /** Prompts written while a turn runs, sent in order when it ends. */
  readonly queue = signal<string[]>([]);

  readonly modelName = computed(() => {
    const model = this.model();
    return model ? modelLabel(model) : 'No model selected';
  });
  readonly actionCount = computed(() => this.timeline().filter((entry) => entry.kind === 'tool').length);

  private nextId = 1;
  private readonly listeners = new Map<string, Set<(params: any) => void>>();
  private generation = 0;
  private pending = new Map<number, Pending>();
  // A new stretch of streamed text starts after every action, so what the
  // model says after acting appears below the action, not above it.
  private segment = 0;

  async boot(): Promise<void> {
    if (!inTauri()) {
      if (new URLSearchParams(location.search).has('demo')) {
        playDemo(this);
        return;
      }
      this.coreState.set('error');
      this.coreError.set('Open this interface through the PWR app (npm run tauri dev).');
      return;
    }
    await bridge.onMessage((message) => this.receive(message));
    await bridge.onLog((line) => this.logs.update((lines) => [...lines.slice(-400), line]));
    await bridge.onExit((generation) => {
      // A core this app already replaced; the running one is unaffected.
      if (generation !== this.generation) return;
      this.coreState.set('stopped');
      this.turnActive.set(false);
      for (const waiting of this.pending.values()) waiting.reject(new Error('the core stopped'));
      this.pending.clear();
    });
    try {
      const remembered = await bridge.defaultWorkspace();
      const trusted = await bridge.workspaceIsTrusted(remembered);
      await this.openWorkspace(trusted ? remembered : await bridge.chatHome());
    } catch (error) {
      this.coreState.set('error');
      this.coreError.set(String(error));
    }
  }

  async openWorkspace(path: string): Promise<void> {
    this.coreState.set('starting');
    this.coreError.set('');
    this.sessionId.set(null);
    this.timeline.set([]);
    this.changes.set([]);
    try {
      // Requests to the core being replaced will never be answered.
      for (const waiting of this.pending.values()) waiting.reject(new Error('the workspace changed'));
      this.pending.clear();
      // No core is current while one is being replaced: the old one's exit
      // can arrive before the new one's number does.
      this.generation = -1;
      const started = await bridge.start(path);
      this.generation = started.generation;
      this.workspace.set(started.workspace);
      this.corePath.set(started.core);
      const hello = await this.request('initialize', {
        protocolVersion: 1,
        clientCapabilities: {},
        clientInfo: { name: 'pwr-desktop', version: '0.1.0' },
      });
      this.chatHome.set(hello?._meta?.poorai?.chatHome ?? '');
      this.coreState.set('ready');
      await Promise.all([this.refreshModels(), this.refreshSessions(), this.refreshPermissions()]);
    } catch (error) {
      this.coreState.set('error');
      this.coreError.set(String(error));
    }
  }

  async chooseWorkspace(): Promise<void> {
    const folder = await bridge.pickFolder('Open a workspace');
    if (!folder) return;
    try {
      if (!(await bridge.workspaceIsTrusted(folder))) {
        if (!(await bridge.confirmWorkspaceTrust(folder))) return;
        await bridge.trustWorkspace(folder);
      }
      await this.openWorkspace(folder);
    } catch (error) {
      this.coreError.set(String(error));
    }
  }

  /**
   * Chat mode: a conversation with no workspace, which reads only the files,
   * folders and images attached to it. The model in use comes along.
   */
  async openChat(): Promise<void> {
    if (this.chatMode() || !this.chatHome() || this.turnActive()) return;
    this.lastWorkspace = this.workspace();
    const model = this.model();
    await this.openWorkspace(this.chatHome());
    if (!this.model() && model) await this.selectModel(model);
  }

  /** Back to the workspace chat mode was opened from, or a new one. */
  async leaveChat(): Promise<void> {
    if (this.turnActive()) return;
    if (this.lastWorkspace && this.lastWorkspace !== this.chatHome()) {
      await this.openWorkspace(this.lastWorkspace);
    } else {
      await this.chooseWorkspace();
    }
  }

  // ---------------------------------------------------------------- models

  async refreshModels(params: Record<string, unknown> = {}): Promise<void> {
    const reply = await this.request('_poorai/models', { cwd: this.workspace(), ...params });
    this.models.set(reply.installed ?? []);
    this.visionModels.set(reply.vision ?? []);
    this.model.set(reply.model ?? null);
    this.reasoningEffort.set(reply.reasoningEffort ?? 'medium');
    this.reasoning.set(reply.reasoning ?? null);
    this.compatibility.set(reply.compatibility ?? null);
    this.backendNote.set(reply.backendUnavailable ?? '');
    const decision = reply.contextDecision ?? {};
    this.context.set({
      tokens: reply.contextTokens ?? 0,
      rationale: decision.rationale ?? '',
      ceiling: decision.bindingCeiling ?? '',
      setting: decision.setting ?? null,
    });
  }

  // ----------------------------------------------------------- permissions

  async refreshPermissions(params: Record<string, unknown> = {}): Promise<void> {
    const reply = await this.request('_poorai/approvals', { cwd: this.workspace(), ...params });
    this.permissionMode.set(reply.mode === 'auto' ? 'auto' : 'ask');
    this.asking.set(reply.asking ?? []);
    this.sandboxed.set(reply.sandboxed !== false);
  }

  /** Switches between asking and running with every permission; saved in the workspace. */
  setPermissionMode(mode: 'ask' | 'auto'): Promise<void> {
    return this.refreshPermissions({ mode });
  }

  selectModel(ref: string): Promise<void> {
    return this.refreshModels({ model: ref });
  }

  setReasoningEffort(effort: ReasoningEffort): Promise<void> {
    return this.refreshModels({ reasoningEffort: effort });
  }

  /** "Use Conservative Defaults": remembered, so the choice is not offered again. */
  useConservativeDefaults(): Promise<void> {
    return this.refreshModels({ acknowledgeProvisional: true });
  }

  /** Quick Calibration of the selected model: bounded, cancellable, local. */
  async quickCalibrate(): Promise<void> {
    if (this.calibrating() || !this.model()) return;
    const calibrationId = crypto.randomUUID();
    this.calibrationId = calibrationId;
    this.calibrationError.set('');
    this.calibrationProgress.set(null);
    this.calibrating.set(true);
    const stop = this.on('_poorai/calibration_progress', (params) => {
      if (params.calibrationId === calibrationId)
        this.calibrationProgress.set({ step: params.step, total: params.total, name: params.name });
    });
    try {
      const reply = await this.request('_poorai/quick_calibration', {
        cwd: this.workspace(),
        calibrationId,
      });
      if (reply.assessment) this.compatibility.set(reply.assessment);
      if (reply.reasoning) this.reasoning.set(reply.reasoning);
      await this.refreshModels();
    } catch (error) {
      this.calibrationError.set(String(error).replace(/^Error: /, ''));
    } finally {
      stop();
      this.calibrating.set(false);
      this.calibrationProgress.set(null);
      this.calibrationId = null;
    }
  }

  cancelQuickCalibration(): void {
    if (this.calibrationId)
      this.notify('_poorai/quick_calibration_cancel', { calibrationId: this.calibrationId });
  }

  stepContext(direction: 1 | -1): Promise<void> {
    const current = this.context()?.tokens ?? 8192;
    const next =
      direction > 0
        ? CONTEXT_STEPS.find((value) => value > current)
        : [...CONTEXT_STEPS].reverse().find((value) => value < current);
    return next ? this.refreshModels({ contextTokens: next }) : Promise.resolve();
  }

  async revertChange(change: FileDiff): Promise<void> {
    await bridge.restoreFile(this.workspace(), change.path, change.oldText, !!change.created);
    this.changes.update((all) => all.filter((item) => item.path !== change.path));
  }

  async revertAllChanges(): Promise<void> {
    for (const change of [...this.changes()].reverse()) await this.revertChange(change);
  }

  // --------------------------------------------------------------- context

  /**
   * The context panel's account: window, what fills it (estimated), the
   * auto-compaction threshold and the last compaction. Nothing to read
   * before a conversation exists.
   */
  async refreshContext(params: Record<string, unknown> = {}): Promise<void> {
    const sessionId = this.sessionId();
    if (!sessionId) {
      this.contextInfo.set(null);
      return;
    }
    this.contextInfo.set(await this.request('_poorai/context', { sessionId, ...params }));
  }

  /** Sets the share of the window at which the conversation compacts itself. */
  setAutoCompact(percent: number): Promise<void> {
    return this.refreshContext({ autoCompactPercent: percent });
  }

  /**
   * "Compact now": the same compaction the conversation performs by itself,
   * asked for between turns.
   */
  async compactNow(): Promise<void> {
    const sessionId = this.sessionId();
    if (!sessionId || this.turnActive() || this.compacting()) return;
    this.compacting.set(true);
    try {
      const reply = await this.request('_poorai/compact', { sessionId });
      if (!reply.compacted) this.notice('Nothing to compact', reply.reason ?? 'The conversation is already small.', 'info');
      else this.usage.set(null);
      await this.refreshContext();
    } catch (error) {
      this.notice('Compaction failed', String(error), 'error');
    } finally {
      this.compacting.set(false);
    }
  }

  private onCompacted(params: any): void {
    if (params.sessionId && params.sessionId !== this.sessionId()) return;
    const manual = params.trigger === 'manual';
    this.notice(
      manual ? 'Context compacted' : 'Context compacted automatically',
      `${String(params.note ?? '').replace(/^summarised/, 'Summarised')}. The instructions, the task, files changed, open errors and the last check result were kept.`,
      'info',
    );
    if (this.contextInfo()) void this.refreshContext();
  }

  // ------------------------------------------------------------ extensions

  /** A `_poorai/*` request for the other stores (the Model Manager). */
  call(method: string, params: unknown): Promise<any> {
    return this.request(method, params);
  }

  /** A notification the core sends outside a turn, such as download progress. */
  on(method: string, listener: (params: any) => void): () => void {
    const set = this.listeners.get(method) ?? new Set();
    set.add(listener);
    this.listeners.set(method, set);
    return () => set.delete(listener);
  }

  /** A notification to the core, which answers nothing. */
  notify(method: string, params: unknown): void {
    void bridge.send({ jsonrpc: '2.0', method, params });
  }

  // -------------------------------------------------------------- sessions

  async refreshSessions(): Promise<void> {
    const reply = await this.request('session/list', { cwd: this.workspace() });
    this.sessions.set(reply.sessions ?? []);
  }

  async deleteConversation(sessionId: string): Promise<void> {
    if (this.turnActive()) return;
    if (this.demoAnswer) {
      this.sessions.update((sessions) => sessions.filter((session) => session.sessionId !== sessionId));
      if (this.sessionId() === sessionId) this.newConversation();
      return;
    }
    await this.request('_poorai/session_delete', { cwd: this.workspace(), sessionId });
    if (this.sessionId() === sessionId) this.newConversation();
    await this.refreshSessions();
  }

  newConversation(): void {
    if (this.turnActive()) return;
    this.usage.set(null);
    this.contextInfo.set(null);
    this.sessionId.set(null);
    this.timeline.set([]);
    this.changes.set([]);
    this.outcome.set('');
  }

  async resume(sessionId: string): Promise<void> {
    if (this.turnActive()) return;
    this.timeline.set([]);
    this.changes.set([]);
    this.segment = 0;
    this.usage.set(null);
    this.contextInfo.set(null);
    this.sessionId.set(sessionId);
    await this.request('session/load', { sessionId, cwd: this.workspace(), mcpServers: [] });
  }

  // ------------------------------------------------------------ the turn

  async send(text: string): Promise<void> {
    const prompt = text.trim();
    if (!prompt) return;
    if (this.turnActive()) {
      this.queue.update((queued) => [...queued, prompt]);
      return;
    }
    let sessionId = this.sessionId();
    if (!sessionId) {
      const created = await this.request('session/new', { cwd: this.workspace(), mcpServers: [] });
      sessionId = created.sessionId as string;
      this.sessionId.set(sessionId);
    }
    const attachments = this.attachments();
    this.attachments.set([]);
    this.push({
      key: `user-${Date.now()}`,
      kind: 'user',
      title: 'You',
      text: prompt,
      status: 'sent',
      attachments,
      at: Date.now(),
    });
    this.segment += 1;
    this.turnActive.set(true);
    this.outcome.set('');
    this.lastEventAt.set(Date.now());
    const blocks: unknown[] = [{ type: 'text', text: prompt }];
    for (const path of attachments) {
      blocks.push({ type: 'resource_link', uri: fileUri(path), name: basename(path) });
    }
    try {
      const reply = await this.request('session/prompt', {
        sessionId,
        prompt: blocks,
        // Chat mode has nothing to keep working on.
        goalMode: this.goalMode() && !this.chatMode(),
      });
      this.finish(reply);
    } catch (error) {
      this.notice('The turn failed', String(error), 'error');
    } finally {
      this.turnActive.set(false);
      this.settleLive();
      void this.refreshSessions();
      this.sendNextQueued();
    }
  }

  /** The next queued prompt, once the turn it waited for has ended. */
  private sendNextQueued(): void {
    const [next, ...rest] = this.queue();
    if (next === undefined) return;
    this.queue.set(rest);
    setTimeout(() => void this.send(next), 0);
  }

  unqueue(index: number): void {
    this.queue.update((queued) => queued.filter((_, position) => position !== index));
  }

  /**
   * A queued prompt delivered into the running turn: the core hands it to the
   * model at the next safe point, between two actions, instead of after the
   * turn ends.
   */
  async steerNow(index: number): Promise<void> {
    const text = this.queue()[index];
    const sessionId = this.sessionId();
    if (text === undefined || !sessionId || !this.turnActive()) return;
    this.unqueue(index);
    this.push({ key: `user-${Date.now()}`, kind: 'user', title: 'You', text, status: 'sent', at: Date.now() });
    this.segment += 1;
    try {
      await this.request('_poorai/steer', { sessionId, text });
    } catch (error) {
      this.notice('The message could not be delivered', String(error), 'error');
    }
  }

  cancel(): void {
    const sessionId = this.sessionId();
    if (sessionId && this.turnActive()) {
      void bridge.send({ jsonrpc: '2.0', method: 'session/cancel', params: { sessionId } });
    }
  }

  async runCommand(name: 'changes' | 'verify' | 'report' | 'diagnose' | 'doctor'): Promise<void> {
    const sessionId = this.sessionId();
    if (!sessionId) {
      this.commandOutput.set({ name, text: 'Start a conversation first.' });
      return;
    }
    this.commandOutput.set({ name, text: 'Running…' });
    try {
      const reply = await this.request(`_poorai/${name}`, { sessionId });
      this.commandOutput.set({ name, text: reply.text ?? JSON.stringify(reply, null, 2) });
    } catch (error) {
      this.commandOutput.set({ name, text: String(error) });
    }
  }

  answerPermission(optionId: string): void {
    const asked = this.permission();
    if (!asked) return;
    this.permission.set(null);
    void bridge.send({
      jsonrpc: '2.0',
      id: asked.id,
      result: { outcome: { outcome: 'selected', optionId } },
    });
  }

  async attachFiles(): Promise<void> {
    const files = await bridge.pickFiles();
    this.attachments.update((current) => unique([...current, ...files]));
  }

  async attachImages(): Promise<void> {
    const images = await bridge.pickImages();
    this.attachments.update((current) => unique([...current, ...images]));
  }

  async attachFolder(): Promise<void> {
    const folder = await bridge.pickFolder('Attach a folder as a read-only reference');
    if (folder) this.attachments.update((current) => unique([...current, folder]));
  }

  detach(path: string): void {
    this.attachments.update((current) => current.filter((item) => item !== path));
  }

  // -------------------------------------------------------- the protocol

  /** Canned answers for the browser demo (`?demo`), which has no core. */
  private demoAnswer?: (method: string, params: any) => any;

  useDemo(answer: (method: string, params: any) => any): void {
    this.demoAnswer = answer;
  }

  private request(method: string, params: unknown): Promise<any> {
    if (this.demoAnswer) {
      const answer = this.demoAnswer;
      return new Promise((resolve, reject) =>
        setTimeout(() => {
          try {
            resolve(answer(method, params));
          } catch (error) {
            reject(error);
          }
        }, 350),
      );
    }
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      bridge.send({ jsonrpc: '2.0', id, method, params }).catch((error) => {
        this.pending.delete(id);
        reject(error);
      });
    });
  }

  /** Every message from the core; public so tests can feed it one. */
  receive(message: any): void {
    this.lastEventAt.set(Date.now());
    if (message.method && message.id !== undefined) {
      this.serverRequest(message);
      return;
    }
    if (message.method === '_poorai/usage') {
      const { used, window } = message.params ?? {};
      if (typeof used === 'number' && typeof window === 'number' && window > 0) {
        this.usage.set({ used, window });
      }
      return;
    }
    if (message.method === '_poorai/compacted') {
      this.onCompacted(message.params ?? {});
      return;
    }
    if (message.method === 'session/update') {
      this.apply(message.params?.update ?? {});
      return;
    }
    if (message.method && message.id === undefined) {
      for (const listener of this.listeners.get(message.method) ?? []) listener(message.params ?? {});
      return;
    }
    if (message.id !== undefined && this.pending.has(message.id)) {
      const waiting = this.pending.get(message.id)!;
      this.pending.delete(message.id);
      if (message.error) waiting.reject(new Error(message.error.message ?? 'request failed'));
      else waiting.resolve(message.result ?? {});
    }
  }

  private serverRequest(message: any): void {
    if (message.method === 'session/request_permission') {
      this.permission.set({
        id: message.id,
        title: message.params?.toolCall?.title ?? 'An action needs your permission',
        approval: message.params?._meta?.poorai?.approval ?? '',
        options: message.params?.options ?? [],
      });
      return;
    }
    void bridge.send({
      jsonrpc: '2.0',
      id: message.id,
      error: { code: -32601, message: `${message.method} is not supported by this client` },
    });
  }

  private apply(update: any): void {
    const kind: string = update.sessionUpdate ?? '';
    const text: string = update.content?.text ?? '';
    const live = update._meta?.poorai?.live === true;
    switch (kind) {
      case 'agent_thought_chunk':
        this.stream(`thought-${this.segment}`, 'thought', 'Thinking', text);
        return;
      case 'agent_message_chunk':
        if (live) {
          this.stream(`reply-${this.segment}`, 'reply', 'PWR', text);
        } else {
          this.finalMessage(text);
        }
        return;
      case 'user_message_chunk':
        // A replayed prompt carries what the core appended to it (goal-mode
        // instructions, attachments); the person's own words come first.
        this.push({ key: `user-${Date.now()}-${Math.random()}`, kind: 'user', title: 'You', text: ownWords(text), status: 'sent', at: Date.now() });
        this.segment += 1;
        return;
      case 'tool_call':
        this.upsertTool(update, 'pending');
        return;
      case 'tool_call_update':
        this.upsertTool(update, update.status ?? 'in_progress');
        return;
      default:
        return;
    }
  }

  private upsertTool(update: any, status: string): void {
    const key: string = update.toolCallId ?? `tool-${Date.now()}`;
    const detail: string | undefined = update._meta?.poorai?.detail;
    const diffBlock = (update.content ?? []).find?.((block: any) => block.type === 'diff');
    const failure = (update.content ?? []).find?.((block: any) => block.type === 'content')?.content?.text;
    const diff: FileDiff | undefined = diffBlock
      ? { path: relative(diffBlock.path, this.workspace()), oldText: diffBlock.oldText ?? '', newText: diffBlock.newText ?? '', created: diffBlock.oldText == null }
      : undefined;
    if (diff) {
      // Most recent first; a file touched again moves back to the top, keeping
      // the state it started from so its diff covers the whole conversation.
      this.changes.update((all) => {
        const earlier = all.find((item) => item.path === diff.path);
        const merged = earlier ? { ...diff, oldText: earlier.oldText, created: earlier.created ?? diff.created } : diff;
        return [merged, ...all.filter((item) => item.path !== diff.path)];
      });
    }
    // Any action new to the timeline -- a refused call arrives only as an
    // update, never as a `tool_call` -- starts a new stretch of text, or the
    // next step's words were appended to the previous step's.
    if (!this.timeline().some((entry) => entry.key === key)) {
      this.settleLive();
      this.segment += 1;
    }
    this.timeline.update((entries) => {
      const index = entries.findIndex((entry) => entry.key === key);
      const previous = index >= 0 ? entries[index] : undefined;
      const next: Entry = {
        key,
        kind: 'tool',
        title: update.title ?? previous?.title ?? 'Action',
        text: failure ? explain(failure) : detail ?? previous?.text ?? '',
        status: toolStatus(status),
        toolKind: update.kind ?? previous?.toolKind,
        diff: diff ?? previous?.diff,
        at: previous?.at ?? Date.now(),
      };
      if (index < 0) return [...entries, next];
      const copy = entries.slice();
      copy[index] = next;
      return copy;
    });
  }

  private stream(key: string, kind: 'thought' | 'reply', title: string, delta: string): void {
    if (!delta) return;
    this.timeline.update((entries) => {
      const index = entries.findIndex((entry) => entry.key === key);
      if (index < 0) {
        return [...entries, { key, kind, title, text: delta.replace(/^\s+/, ''), status: 'live', at: Date.now() }];
      }
      const copy = entries.slice();
      copy[index] = { ...copy[index], text: copy[index].text + delta };
      return copy;
    });
  }

  /** The whole answer at a turn's end: settle the streamed copy, or add it. */
  private finalMessage(text: string): void {
    if (!text.trim()) return;
    if (/^Checkpoint after \d+ action/.test(text)) {
      this.notice('Checkpoint', text, 'info');
      return;
    }
    const entries = this.timeline();
    const live = [...entries].reverse().find((entry) => entry.kind === 'reply' && entry.status === 'live');
    if (live && live.text.trim() === text.trim()) {
      this.settleLive();
      return;
    }
    this.settleLive();
    this.push({ key: `reply-final-${Date.now()}-${Math.random()}`, kind: 'reply', title: 'PWR', text, status: 'done', at: Date.now() });
  }

  private finish(reply: any): void {
    const meta = reply?._meta?.poorai ?? {};
    const goal = meta.goal;
    const actions = meta.totalActions ?? meta.actions ?? 0;
    const lines: string[] = [];
    if (goal?.verified) lines.push(`Goal verified by the declared acceptance checks after ${actions} actions.`);
    else if (goal?.guardReached) lines.push(`Goal mode paused after ${actions} actions without a verified completion.`);
    else if (goal?.needsAcceptance) lines.push('Technical checks passed; no acceptance contract was declared, so the goal is not verified.');
    else if (meta.terminal === 'budget') lines.push(`Paused after ${actions} actions to check in. Send "carry on" to continue.`);
    else if (reply?.stopReason === 'cancelled' || meta.terminal === 'interrupted') lines.push('Stopped.');
    else if (meta.terminal === 'protocol')
      lines.push(`Stopped after ${actions} actions: the model's last replies could not be read as actions. Send "carry on" to retry.`);
    else if (meta.terminal === 'provider')
      lines.push(`Stopped after ${actions} actions: the engine failed. Send "carry on" once it is serving again.`);
    else if (meta.terminal === 'recovery')
      lines.push(`Stopped after ${actions} actions: the work was not moving (repeated steps or a full context).`);
    else if (meta.terminal === 'declined') lines.push('The model declined the task.');
    else lines.push(`Finished after ${actions} action${actions === 1 ? '' : 's'}.`);
    this.outcome.set(lines.join(' '));
  }

  private notice(title: string, text: string, status: 'info' | 'error'): void {
    this.settleLive();
    this.push({ key: `notice-${Date.now()}-${Math.random()}`, kind: 'notice', title, text, status, at: Date.now() });
  }

  private settleLive(): void {
    this.timeline.update((entries) =>
      entries.some((entry) => entry.status === 'live')
        ? entries.map((entry) => (entry.status === 'live' ? { ...entry, status: 'done' } : entry))
        : entries,
    );
  }

  private push(entry: Entry): void {
    this.timeline.update((entries) => [...entries, entry]);
  }
}

function ownWords(text: string): string {
  const cut = ['\n\nGoal mode is enabled.', '\n--- Attachments for this task only ---']
    .map((marker) => text.indexOf(marker))
    .filter((index) => index > 0);
  return cut.length ? text.slice(0, Math.min(...cut)).trimEnd() : text;
}

function toolStatus(status: string): Entry['status'] {
  switch (status) {
    case 'pending':
      return 'pending';
    case 'in_progress':
      return 'running';
    case 'completed':
      return 'done';
    case 'failed':
      return 'failed';
    default:
      return 'running';
  }
}

function explain(raw: string): string {
  try {
    const parsed = JSON.parse(raw);
    return parsed.denied ?? parsed.error ?? parsed.message ?? raw;
  } catch {
    return raw;
  }
}

function fileUri(path: string): string {
  return 'file://' + path.split('/').map(encodeURIComponent).join('/');
}

function basename(path: string): string {
  return path.split('/').filter(Boolean).pop() ?? path;
}

function relative(path: string, workspace: string): string {
  return workspace && path.startsWith(workspace + '/') ? path.slice(workspace.length + 1) : path;
}

function unique(items: string[]): string[] {
  return [...new Set(items)];
}
