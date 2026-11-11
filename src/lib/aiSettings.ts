//! AI 设置页的状态机（M11）。
//!
//! 负责三件事：把读取到的状态与用户正在编辑的草稿分开、把所有写入放进同一条
//! 串行队列（地址先于密钥）、给状态与模型列表请求加上代际标记，避免迟到响应
//! 覆盖新输入、新地址或新会话。组件只渲染 `view()`，IPC 通过 `transport` 注入。

import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import type { AiLanguage, AiModelList, AiStatus, SettingsPatch } from "@/lib/ipc";

export type AiDraftField = "baseUrl" | "model" | "language" | "prompt";

export interface AiDraft {
  baseUrl: string;
  model: string;
  language: AiLanguage;
  prompt: string;
}

export interface AiModelListView {
  state: "idle" | "loading" | "ready" | "empty" | "error";
  items: string[];
  error: AppErrorPayload | null;
}

export interface AiSettingsView {
  /** 当前编辑值。 */
  draft: AiDraft;
  /** 已保存的基准值（Rust 规范化后的地址与模型）。 */
  saved: AiDraft;
  configured: boolean;
  keySet: boolean;
  notice: "keySet" | "keyUnset" | null;
  error: AppErrorPayload | null;
  models: AiModelListView;
}

export interface AiSettingsTransport {
  status(): Promise<AiStatus>;
  /** 返回 `null` 表示保存成功。 */
  save(patch: SettingsPatch): Promise<AppErrorPayload | null>;
  setKey(key: string, baseUrl: string): Promise<AiStatus>;
  clearKey(baseUrl: string): Promise<AiStatus>;
  listModels(baseUrl: string): Promise<AiModelList>;
}

export interface AiSettingsDeps {
  transport: AiSettingsTransport;
}

const FIELDS: AiDraftField[] = ["baseUrl", "model", "language", "prompt"];

function emptyDraft(): AiDraft {
  return { baseUrl: "", model: "", language: "repo", prompt: "" };
}

function idleModels(): AiModelListView {
  return { state: "idle", items: [], error: null };
}

/** 与 Rust `normalize_base_url` 对齐：去首尾空白与末尾斜杠。 */
function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, "");
}

function setDraftField(target: AiDraft, field: AiDraftField, value: string): void {
  if (field === "language") target.language = value as AiLanguage;
  else target[field] = value;
}

function notConfiguredError(): AppErrorPayload {
  return {
    kind: "rejected",
    code: "errors.ai.notConfigured",
    params: {},
    message: t("errors.ai.notConfigured"),
  };
}

export class AiSettings {
  private readonly deps: AiSettingsDeps;
  private readonly listeners = new Set<() => void>();
  private saved: AiDraft = emptyDraft();
  private draft: AiDraft = emptyDraft();
  private touched: Record<AiDraftField, boolean> = {
    baseUrl: false,
    model: false,
    language: false,
    prompt: false,
  };
  private defaultPrompt = "";
  private keySet = false;
  private notice: AiSettingsView["notice"] = null;
  private error: AppErrorPayload | null = null;
  private models: AiModelListView = idleModels();
  private isOpen = false;
  private sessionEpoch = 0;
  private statusEpoch = 0;
  private modelsEpoch = 0;
  /** 从接收密钥意图直到全部排队操作结束，模型请求都不能使用中间凭据。 */
  private pendingKeyOperations = 0;
  /** 所有写入（设置字段与密钥）共用一条队列，保证提交顺序与最终值。 */
  private tail: Promise<void> = Promise.resolve();

  constructor(deps: AiSettingsDeps) {
    this.deps = deps;
  }

  /** 订阅界面刷新；控制器本身可以比某个对话框实例活得更久。 */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** 当前页面会话代次；密钥完成回调用它确认仍属于发起操作的页面。 */
  sessionToken(): number {
    return this.sessionEpoch;
  }

  private notify(): void {
    for (const listener of Array.from(this.listeners)) listener();
  }

  view(): AiSettingsView {
    return {
      draft: { ...this.draft },
      saved: { ...this.saved },
      configured: this.saved.baseUrl !== "" && this.saved.model !== "",
      keySet: this.keySet,
      notice: this.notice,
      error: this.error,
      models: { ...this.models, items: [...this.models.items] },
    };
  }

  /** 进入 AI 页：重置草稿跟踪；等已排队写入完成后再读权威状态。 */
  open(): void {
    this.isOpen = true;
    this.sessionEpoch += 1;
    const session = this.sessionEpoch;
    this.draft = { ...this.saved };
    this.touched = {
      baseUrl: false,
      model: false,
      language: false,
      prompt: false,
    };
    this.notice = null;
    this.error = null;
    this.models = idleModels();
    this.modelsEpoch += 1;
    const epoch = ++this.statusEpoch;
    this.notify();
    const pending = this.tail;
    void pending.then(() => {
      if (this.isOpen && session === this.sessionEpoch) void this.loadStatus(epoch, session);
    });
  }

  /** 离开 AI 页：让所有在途读取失效；已排队的写入仍会执行。 */
  close(): void {
    this.isOpen = false;
    this.sessionEpoch += 1;
    this.statusEpoch += 1;
    this.modelsEpoch += 1;
    this.touched = {
      baseUrl: false,
      model: false,
      language: false,
      prompt: false,
    };
    this.notice = null;
    this.error = null;
    this.models = idleModels();
    this.notify();
  }

  edit(field: AiDraftField, value: string): void {
    this.draft = { ...this.draft, [field]: value } as AiDraft;
    this.touched = { ...this.touched, [field]: true };
    this.error = null;
    if (field === "baseUrl") {
      // 地址一变，旧列表就不再代表当前服务。
      this.modelsEpoch += 1;
      this.models = idleModels();
    }
    this.notify();
  }

  /**
   * 提交一个字段。调用时的值进入串行队列，执行时再与“当前已保存值”比较，
   * 因此 `save(B)` 未完成时 `edit/save(A)` 不会被旧基准误判为无需保存。
   */
  save(field: AiDraftField): Promise<boolean> {
    const sent = this.normalized(field);
    const draftAtCall = this.draft[field];
    const session = this.sessionEpoch;
    return this.enqueue(() => this.saveNow(field, sent, draftAtCall, session));
  }

  /** 语言下拉没有单独的保存按钮：选择后立即提交。 */
  selectLanguage(language: AiLanguage): Promise<boolean> {
    this.edit("language", language);
    return this.save("language");
  }

  /** 从列表选择模型：先填入输入框，再按普通字段保存。 */
  selectModel(model: string): Promise<boolean> {
    this.edit("model", model);
    return this.save("model");
  }

  /** 把提示词恢复为默认值（保存空字符串表示使用默认）。 */
  resetPrompt(): Promise<boolean> {
    this.draft = { ...this.draft, prompt: this.defaultPrompt };
    this.touched = { ...this.touched, prompt: true };
    return this.save("prompt");
  }

  /**
   * 保存密钥。点击时捕获服务身份；等待地址保存期间地址或页面变化则失效，
   * 不会按随后变化的地址写入。密钥写入与设置写入在同一条队列里。
   */
  saveKey(key: string): Promise<boolean> {
    const trimmed = key.trim();
    if (!trimmed) return Promise.resolve(false);
    return this.queueKeyOperation(
      (baseUrl) => this.deps.transport.setKey(trimmed, baseUrl),
      "keySet",
    );
  }

  clearKey(): Promise<boolean> {
    return this.queueKeyOperation((baseUrl) => this.deps.transport.clearKey(baseUrl), "keyUnset");
  }

  private queueKeyOperation(
    apply: (baseUrl: string) => Promise<AiStatus>,
    notice: "keySet" | "keyUnset",
  ): Promise<boolean> {
    this.pendingKeyOperations += 1;
    this.invalidateForKeyIntent();
    const intent = this.normalized("baseUrl");
    const session = this.sessionEpoch;
    return this.enqueue(async () => {
      try {
        return await this.runKeyOperation(intent, session, apply, notice);
      } finally {
        this.pendingKeyOperations -= 1;
        if (this.pendingKeyOperations === 0) this.fetchModels();
      }
    });
  }

  /** 收到密钥意图就使旧列表和旧错误失效，不等队列前面的设置写入结束。 */
  private invalidateForKeyIntent(): void {
    if (!this.isOpen) return;
    this.statusEpoch += 1;
    this.modelsEpoch += 1;
    this.models = idleModels();
    this.error = null;
    this.notice = null;
    this.notify();
  }

  /** 读取当前服务模型列表；只对“已保存且草稿仍一致”的地址发起。 */
  fetchModels(): void {
    const draftUrl = this.normalized("baseUrl");
    const baseUrl = this.saved.baseUrl;
    const epoch = ++this.modelsEpoch;
    if (!this.isOpen || this.pendingKeyOperations > 0 || !baseUrl || draftUrl !== baseUrl) {
      this.models = idleModels();
      this.notify();
      return;
    }
    this.models = { state: "loading", items: [], error: null };
    this.notify();
    void this.loadModels(baseUrl, epoch);
  }

  retryModels(): void {
    this.fetchModels();
  }

  private enqueue<T>(operation: () => Promise<T>): Promise<T> {
    const run = this.tail.then(operation, operation);
    this.tail = run.then(() => undefined, () => undefined);
    return run;
  }

  private async saveNow(
    field: AiDraftField,
    sent: string,
    draftAtCall: string,
    session: number,
  ): Promise<boolean> {
    const sessionMatches = () => session === this.sessionEpoch;
    if (sessionMatches()) {
      // 只有草稿仍是提交时的值才写回规范化结果，避免覆盖之后的新输入。
      if (this.draft[field] === draftAtCall) {
        this.draft = { ...this.draft, [field]: sent } as AiDraft;
      }
      this.touched = { ...this.touched, [field]: true };
    }
    if (sent === this.saved[field]) {
      if (sessionMatches()) {
        this.error = null;
        this.notify();
        // 地址改走又改回（或只加/去末尾斜杠）时，编辑阶段已清空列表；
        // 值虽然不需要写回，仍要恢复当前服务的模型入口。
        if (field === "baseUrl" && this.isOpen && this.models.state === "idle") {
          this.fetchModels();
        }
      }
      return true;
    }
    const effective = field === "prompt" && sent === this.defaultPrompt ? this.defaultPrompt : sent;
    const previous = this.saved[field];
    const patch = this.patchFor(field, sent);
    if (sessionMatches()) {
      this.error = null;
      this.notify();
    }
    let failure: AppErrorPayload | null = null;
    try {
      failure = await this.deps.transport.save(patch);
    } catch (caught) {
      failure = toAppError(caught);
    }
    if (failure) {
      if (sessionMatches()) {
        this.error = failure;
        this.notify();
      }
      return false;
    }
    // 写入已经发生：无论页面是否已关闭，基准值都要推进，避免下一次保存误判。
    this.saved = { ...this.saved, [field]: effective } as AiDraft;
    if (field === "baseUrl" && effective !== previous) {
      // 地址变化时 Rust 会清掉旧密钥。
      this.keySet = false;
    }
    if (!sessionMatches()) return true;
    if (field === "baseUrl" && effective !== previous) {
      this.modelsEpoch += 1;
      this.models = idleModels();
    }
    const epoch = ++this.statusEpoch;
    try {
      const status = await this.deps.transport.status();
      if (this.isOpen && sessionMatches() && epoch === this.statusEpoch) {
        this.applyStatus(status, { field, sent: effective });
      }
    } catch {
      // 保存已经成功；状态刷新失败时保留刚推进的基准值。
    }
    this.notify();
    if (field === "baseUrl" && this.isOpen) this.fetchModels();
    return true;
  }

  private async runKeyOperation(
    intent: string,
    session: number,
    apply: (baseUrl: string) => Promise<AiStatus>,
    notice: "keySet" | "keyUnset",
  ): Promise<boolean> {
    const sessionMatches = () => session === this.sessionEpoch;
    if (!this.isOpen || !sessionMatches()) return false;
    if (!intent) {
      this.error = notConfiguredError();
      this.notify();
      return false;
    }
    // 点击后地址草稿又变了：这次密钥操作不再对应当前界面，直接失效。
    if (this.normalized("baseUrl") !== intent) return false;
    if (this.saved.baseUrl !== intent) {
      const saved = await this.saveNow("baseUrl", intent, this.draft.baseUrl, session);
      if (!saved) return false;
    }
    if (!this.isOpen || !sessionMatches()) return false;
    if (this.normalized("baseUrl") !== intent || this.saved.baseUrl !== intent) return false;
    const epoch = ++this.statusEpoch;
    try {
      const status = await apply(intent);
      if (this.isOpen && sessionMatches() && epoch === this.statusEpoch) {
        this.error = null;
        this.applyStatus(status);
        this.notice = notice;
        this.notify();
      }
      return true;
    } catch (caught) {
      if (this.isOpen && sessionMatches() && epoch === this.statusEpoch) {
        this.error = toAppError(caught);
        this.notify();
      }
      return false;
    }
  }

  private async loadModels(baseUrl: string, epoch: number): Promise<void> {
    try {
      const reply = await this.deps.transport.listModels(baseUrl);
      if (!this.isOpen || epoch !== this.modelsEpoch || baseUrl !== this.saved.baseUrl) return;
      this.models = reply.models.length
        ? { state: "ready", items: [...reply.models], error: null }
        : { state: "empty", items: [], error: null };
    } catch (caught) {
      if (!this.isOpen || epoch !== this.modelsEpoch || baseUrl !== this.saved.baseUrl) return;
      this.models = { state: "error", items: [], error: toAppError(caught) };
    }
    this.notify();
  }

  private async loadStatus(epoch: number, session: number): Promise<void> {
    try {
      const status = await this.deps.transport.status();
      if (!this.isOpen || session !== this.sessionEpoch || epoch !== this.statusEpoch) return;
      this.applyStatus(status);
      this.notify();
      this.fetchModels();
    } catch (caught) {
      if (!this.isOpen || session !== this.sessionEpoch || epoch !== this.statusEpoch) return;
      this.error = toAppError(caught);
      this.notify();
    }
  }

  /**
   * 应用权威状态：未编辑的字段直接更新；刚保存的字段若草稿仍等于提交值，
   * 也接受 Rust 的规范化结果（例如补上/去掉末尾斜杠）。其他已编辑字段保留。
   */
  private applyStatus(status: AiStatus, savedField?: { field: AiDraftField; sent: string }): void {
    const next: AiDraft = {
      baseUrl: status.baseUrl,
      model: status.model,
      language: status.language,
      prompt: status.prompt,
    };
    const draft = { ...this.draft };
    const touched = { ...this.touched };
    for (const field of FIELDS) {
      if (!touched[field]) {
        setDraftField(draft, field, next[field]);
      } else if (savedField?.field === field && draft[field] === savedField.sent) {
        setDraftField(draft, field, next[field]);
        touched[field] = false;
      }
    }
    this.draft = draft;
    this.touched = touched;
    this.saved = next;
    this.defaultPrompt = status.defaultPrompt;
    this.keySet = status.keySet;
  }

  private normalized(field: AiDraftField): string {
    if (field === "baseUrl") return normalizeBaseUrl(this.draft.baseUrl);
    if (field === "model") return this.draft.model.trim();
    if (field === "language") return this.draft.language;
    return this.draft.prompt;
  }

  private patchFor(field: AiDraftField, value: string): SettingsPatch {
    switch (field) {
      case "baseUrl":
        return { aiBaseUrl: value };
      case "model":
        return { aiModel: value };
      case "language":
        return { aiLanguage: value as AiLanguage };
      case "prompt":
        return { aiPrompt: value === this.defaultPrompt ? "" : value };
    }
  }
}
