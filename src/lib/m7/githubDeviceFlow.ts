import type { DeviceStart } from "../../types/bindings/DeviceStart.ts";
import type { DevicePoll } from "../../types/bindings/DevicePoll.ts";
import type { GithubAccount } from "../../types/bindings/GithubAccount.ts";

export type DeviceFlowView =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "waiting"; device: DeviceStart };

interface DeviceApi {
  githubDeviceStart(): Promise<DeviceStart>;
  githubDevicePoll(flowId: string): Promise<DevicePoll>;
  githubDeviceCancel(flowId: string): Promise<void>;
  githubDeviceOpen(): Promise<void>;
}

interface Callbacks {
  changed(view: DeviceFlowView): void;
  authorized(account: GithubAccount): void;
  failed(error: unknown): void;
  ended(reason: "expired" | "denied"): void;
}

const clock = {
  now: () => Date.now(),
  later: (callback: () => void, delay: number) => setTimeout(callback, delay),
  clear: (timer: ReturnType<typeof setTimeout>) => clearTimeout(timer),
};

/** Own one device authorization attempt, including late replies and timer cleanup. */
export class GithubDeviceFlow {
  private generation = 0;
  private active = false;
  private device: DeviceStart | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private deadline = 0;
  private opening = false;
  private api: DeviceApi;
  private callbacks: Callbacks;
  private time: typeof clock;

  constructor(
    api: DeviceApi,
    callbacks: Callbacks,
    time = clock,
  ) {
    this.api = api;
    this.callbacks = callbacks;
    this.time = time;
  }

  async start(): Promise<void> {
    if (this.active) return;
    this.active = true;
    const generation = ++this.generation;
    this.callbacks.changed({ phase: "starting" });
    try {
      const device = await this.api.githubDeviceStart();
      if (generation !== this.generation) {
        this.release(device.flowId);
        return;
      }
      this.device = device;
      this.deadline = this.time.now() + device.expiresInMs;
      this.callbacks.changed({ phase: "waiting", device });
      this.schedule(generation, device.intervalMs);
      void this.openBrowser();
    } catch (error) {
      if (generation !== this.generation) return;
      this.cancel();
      this.callbacks.failed(error);
    }
  }

  async openBrowser(): Promise<void> {
    if (!this.device || this.opening) return;
    const generation = this.generation;
    this.opening = true;
    try {
      await this.api.githubDeviceOpen();
    } catch (error) {
      // Keep the code and polling alive so the user can retry opening the browser.
      if (generation === this.generation) this.callbacks.failed(error);
    } finally {
      if (generation === this.generation) this.opening = false;
    }
  }

  cancel(notify = true): void {
    ++this.generation;
    if (this.timer !== null) this.time.clear(this.timer);
    this.timer = null;
    if (this.device) this.release(this.device.flowId);
    this.device = null;
    this.active = false;
    this.opening = false;
    if (notify) this.callbacks.changed({ phase: "idle" });
  }

  private release(flowId: string): void {
    void this.api.githubDeviceCancel(flowId).catch(() => undefined);
  }

  private schedule(generation: number, intervalMs: number): void {
    const remaining = Math.max(0, this.deadline - this.time.now());
    this.timer = this.time.later(() => {
      this.timer = null;
      void this.poll(generation);
    }, Math.min(Math.max(intervalMs, 1000), remaining));
  }

  private async poll(generation: number): Promise<void> {
    if (generation !== this.generation || !this.device) return;
    if (this.time.now() >= this.deadline) {
      this.cancel();
      this.callbacks.ended("expired");
      return;
    }
    try {
      const result = await this.api.githubDevicePoll(this.device.flowId);
      if (generation !== this.generation) return;
      if (result.kind === "pending") {
        this.deadline = Math.min(this.deadline, this.time.now() + result.expiresInMs);
        this.schedule(generation, result.intervalMs);
      } else if (result.kind === "authorized") {
        this.cancel();
        this.callbacks.authorized(result.account);
      } else {
        this.cancel();
        this.callbacks.ended(result.kind);
      }
    } catch (error) {
      if (generation !== this.generation) return;
      this.cancel();
      this.callbacks.failed(error);
    }
  }
}
