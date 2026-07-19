import type { SequencedSessionEvent, SessionState } from "./contracts";
import { applySequencedEvent } from "./session";

export class SessionEventCoordinator {
  private current?: SessionState;
  private readonly queue: SequencedSessionEvent[] = [];
  private drainPromise?: Promise<void>;
  private stopped = false;

  constructor(
    private readonly refresh: () => Promise<SessionState>,
    private readonly publish: (state: SessionState) => void,
  ) {}

  async initialize(snapshot: SessionState): Promise<void> {
    if (this.stopped) return;
    this.current = snapshot;
    this.publish(snapshot);
    await this.flush();
  }

  enqueue(envelope: SequencedSessionEvent): void {
    if (this.stopped) return;
    this.queue.push(envelope);
    if (this.current) void this.flush();
  }

  async flush(): Promise<void> {
    if (!this.current || this.stopped) return;
    if (this.drainPromise) return this.drainPromise;
    const draining = this.drain();
    this.drainPromise = draining;
    try {
      await draining;
    } finally {
      if (this.drainPromise === draining) this.drainPromise = undefined;
    }
    if (this.queue.length > 0 && !this.stopped) await this.flush();
  }

  stop(): void {
    this.stopped = true;
    this.queue.length = 0;
  }

  private async drain(): Promise<void> {
    while (this.queue.length > 0 && this.current && !this.stopped) {
      const envelope = this.queue.shift()!;
      const next = await reconcileSessionEnvelope(this.current, envelope, this.refresh);
      if (this.stopped) return;
      if (next !== this.current) {
        this.current = next;
        this.publish(next);
      }
    }
  }
}

export async function reconcileSessionEnvelope(
  current: SessionState,
  envelope: SequencedSessionEvent,
  refresh: () => Promise<SessionState>,
): Promise<SessionState> {
  if (envelope.sessionId === current.sessionId && envelope.sequence <= current.sequence) return current;
  const next = applySequencedEvent(current, envelope);
  if (next) return next;

  const snapshot = await refresh();
  if (envelope.sessionId === snapshot.sessionId && envelope.sequence <= snapshot.sequence) return snapshot;
  const afterRefresh = applySequencedEvent(snapshot, envelope);
  if (afterRefresh) return afterRefresh;

  return refresh();
}
