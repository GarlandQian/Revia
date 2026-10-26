/** Opt-in observations only: fetch completion means response headers, not body/callback completion. */

const IPC_PREFIX = "ipc://localhost/";
const MAX_COMMANDS = 32;
const MAX_CALLBACK_IDS = 512;
const OTHER_COMMAND = "<other>";

export interface IpcFetchCounters {
  started: number;
  headersSettled: number;
  failed: number;
  synchronousThrows: number;
  active: number;
  maxActive: number;
  totalHeaderLatencyMs: number;
  maxHeaderLatencyMs: number;
}

/** Injectable without a WebView; production callers omit this argument. */
export interface IpcDiagnosticsHost {
  transport: { fetch: typeof fetch };
  now(): number;
  callbacks(): unknown;
  domCounts(): { elements: number; diffRows: number };
  resourceTimingCount(): number;
}

function emptyCounters(): IpcFetchCounters {
  return { started: 0, headersSettled: 0, failed: 0, synchronousThrows: 0,
    active: 0, maxActive: 0, totalHeaderLatencyMs: 0, maxHeaderLatencyMs: 0 };
}

function defaultHost(): IpcDiagnosticsHost {
  return {
    transport: window,
    now: () => performance.now(),
    callbacks: () => (window as Window & {
      __TAURI_INTERNALS__?: { callbacks?: unknown };
    }).__TAURI_INTERNALS__?.callbacks,
    domCounts: () => ({ elements: document.querySelectorAll("*").length,
      diffRows: document.querySelectorAll(".diff-canvas > .row").length }),
    resourceTimingCount: () => performance.getEntriesByType("resource").length,
  };
}

function ipcCommand(input: Parameters<typeof fetch>[0]): string | null {
  // Tauri 2.12.4 supplies a string URL. Do not coerce foreign Request/URL objects or read their bodies.
  if (typeof input !== "string" || !input.startsWith(IPC_PREFIX)) return null;
  const path = input.slice(IPC_PREFIX.length).split(/[?#]/, 1)[0];
  if (path.length > 160) return OTHER_COMMAND;
  try {
    return decodeURIComponent(path) || OTHER_COMMAND;
  } catch {
    return OTHER_COMMAND;
  }
}

export function startIpcDiagnostics(host: IpcDiagnosticsHost = defaultHost()) {
  const originalFetch = host.transport.fetch;
  const total = emptyCounters();
  const commands = new Map<string, IpcFetchCounters>();
  const baseline = new Set<number>();
  const observed = new Map<number, number>();
  let baselineRegistrySize: number | null = null;
  let baselineOverflow = 0;
  let observationErrors = 0;
  let commandOverflowRequests = 0;
  let stopped = false;

  function observe<T>(read: () => T): T | null {
    try {
      return read();
    } catch {
      observationErrors += 1;
      return null;
    }
  }

  function now(): number | null {
    return observe(() => {
      const value = host.now();
      if (!Number.isFinite(value)) throw new Error("Invalid diagnostic clock");
      return value;
    });
  }

  function callbackSnapshot(at: number | null) {
    const value = observe(() => host.callbacks());
    if (!(value instanceof Map)) {
      observed.clear();
      return { available: false as const, registrySize: null, baselineRegistrySize,
        baselineTrackedIds: baseline.size, baselineOverflow, baselineStillRegistered: null,
        newIdCount: null, nonBaselineRegistryCount: null, trackedObservedIds: 0,
        observationOverflow: null, oldestObservedAgeMs: null };
    }
    const registry = value as Map<unknown, unknown>;
    if (baselineRegistrySize === null) {
      baselineRegistrySize = registry.size;
      for (const id of registry.keys()) {
        if (typeof id === "number" && baseline.size < MAX_CALLBACK_IDS) baseline.add(id);
      }
      baselineOverflow = registry.size - baseline.size;
    }
    for (const id of observed.keys()) {
      if (!registry.has(id)) observed.delete(id);
    }
    if (at === null) observed.clear();
    let baselineStillRegistered = 0;
    for (const id of baseline) {
      if (registry.has(id)) baselineStillRegistered += 1;
    }
    let nonBaselineRegistryCount = 0;
    for (const id of registry.keys()) {
      if (typeof id === "number" && baseline.has(id)) continue;
      nonBaselineRegistryCount += 1;
      if (typeof id === "number" && at !== null && !observed.has(id)
        && observed.size < MAX_CALLBACK_IDS) observed.set(id, at);
    }
    let oldestObservedAgeMs: number | null = null;
    if (at !== null) {
      oldestObservedAgeMs = 0;
      for (const firstSeen of observed.values()) {
        oldestObservedAgeMs = Math.max(oldestObservedAgeMs, at - firstSeen);
      }
    }
    return { available: true as const, registrySize: registry.size, baselineRegistrySize,
      baselineTrackedIds: baseline.size, baselineOverflow, baselineStillRegistered,
      // An incomplete baseline cannot distinguish a new ID from an untracked startup ID.
      newIdCount: baselineOverflow === 0 ? nonBaselineRegistryCount : null,
      nonBaselineRegistryCount, trackedObservedIds: observed.size,
      observationOverflow: nonBaselineRegistryCount - observed.size, oldestObservedAgeMs };
  }

  callbackSnapshot(now());

  function begin(counter: IpcFetchCounters) {
    counter.started += 1;
    counter.active += 1;
    counter.maxActive = Math.max(counter.maxActive, counter.active);
  }

  function finish(counter: IpcFetchCounters, success: boolean, syncThrow: boolean, latency: number | null) {
    counter.active -= 1;
    if (success) {
      counter.headersSettled += 1;
      if (latency !== null) {
        counter.totalHeaderLatencyMs += latency;
        counter.maxHeaderLatencyMs = Math.max(counter.maxHeaderLatencyMs, latency);
      }
    } else {
      counter.failed += 1;
      if (syncThrow) counter.synchronousThrows += 1;
    }
  }

  const wrappedFetch: typeof fetch = function (this: unknown, ...args: Parameters<typeof fetch>) {
    const command = stopped ? null : ipcCommand(args[0]);
    if (command === null) return Reflect.apply(originalFetch, this, args);
    let key = command;
    const namedCommandCount = commands.size - (commands.has(OTHER_COMMAND) ? 1 : 0);
    if (key === OTHER_COMMAND || (!commands.has(key) && namedCommandCount >= MAX_COMMANDS)) {
      key = OTHER_COMMAND;
      commandOverflowRequests += 1;
    }
    let counter = commands.get(key);
    if (!counter) {
      counter = emptyCounters();
      commands.set(key, counter);
    }
    begin(total);
    begin(counter);
    const startedAt = now();
    const settled = (success: boolean, syncThrow = false) => {
      const endedAt = now();
      const latency = startedAt !== null && endedAt !== null ? Math.max(0, endedAt - startedAt) : null;
      finish(total, success, syncThrow, latency);
      finish(counter, success, syncThrow, latency);
    };
    let promise: ReturnType<typeof fetch>;
    try {
      promise = Reflect.apply(originalFetch, this, args);
    } catch (error) {
      settled(false, true);
      throw error;
    }
    // Return the original promise/Response. These observers neither read the body nor rethrow rejection.
    observe(() => {
      void promise.then(() => settled(true), () => settled(false));
    });
    return promise;
  };

  host.transport.fetch = wrappedFetch;

  return {
    snapshot() {
      const at = now();
      const callbacks = callbackSnapshot(at);
      const dom = observe(() => host.domCounts());
      const resourceTimingEntries = observe(() => host.resourceTimingCount());
      return {
        observedAtMs: at, stopped, fetchHookStillInstalled: host.transport.fetch === wrappedFetch,
        fetch: { completionBoundary: "response headers, not body/callback completion",
          stringIpcUrlsOnly: true, total: { ...total },
          commands: Object.fromEntries([...commands].map(([command, counter]) => [command, { ...counter }])),
          namedCommandLimit: MAX_COMMANDS, commandOverflowRequests },
        callbacks: { ...callbacks, idLimit: MAX_CALLBACK_IDS,
          baselineStillRegisteredIsLowerBound: baselineOverflow > 0,
          ageMeaning: "lower bound from consecutive observations, not exact registration time",
          countsAreNotRequestCounts: true },
        dom, resourceTimingEntries, observationErrors,
      };
    },
    stop() {
      stopped = true;
      if (host.transport.fetch === wrappedFetch) host.transport.fetch = originalFetch;
    },
  };
}
