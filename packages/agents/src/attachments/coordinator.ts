/** All writes to the shared workspace must pass through this coordinator. */
export class WriteCoordinator {
  private active = 0;
  private paused = false;
  private failure: Error | undefined;
  private waiters = new Set<() => void>();

  private changed(): void { for (const wake of this.waiters) wake(); this.waiters.clear(); }
  private wait(): Promise<void> { return new Promise(resolve => this.waiters.add(resolve)); }

  async run<T>(operation: () => Promise<T>): Promise<T> {
    while (this.paused && !this.failure) await this.wait();
    if (this.failure) throw this.failure;
    this.active++;
    try { return await operation(); }
    finally { this.active--; this.changed(); }
  }

  async handoff<T>(operation: (signal: AbortSignal) => Promise<T>, timeoutMs: number): Promise<T> {
    if (this.paused || this.failure) throw new Error('Writer is already held or requires reconciliation');
    this.paused = true;
    const controller = new AbortController();
    try {
      const result = await bounded((async () => {
        while (this.active && !controller.signal.aborted) await this.wait();
        controller.signal.throwIfAborted();
        return operation(controller.signal);
      })(), timeoutMs, 'Writer handoff timed out; keep writes stopped until reconciliation');
      this.paused = false; this.changed();
      return result;
    } catch (error) {
      controller.abort();
      this.failure = error instanceof Error ? error : new Error(String(error));
      this.changed(); throw this.failure;
    }
  }

  async reconcile(operation: () => Promise<void>): Promise<void> {
    if (!this.paused || this.active) throw new Error('Reconciliation requires a stopped writer with no in-flight tools');
    await operation();
    this.failure = undefined; this.paused = false; this.changed();
  }
}

export async function bounded<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([promise, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error(message)), timeoutMs); })]);
  } finally { if (timer) clearTimeout(timer); }
}
