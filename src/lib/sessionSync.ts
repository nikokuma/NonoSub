import type { SequencedSessionEvent, SessionState } from "./contracts";
import { applySequencedEvent } from "./session";

export class SessionEventCoordinator {
  private current?: SessionState;
  private readonly queue: SequencedSessionEvent[] = [];
  private drainPromise?: Promise<void>;
  private stopped = false;
  private replacementVersion = 0;

  constructor(
    private readonly refresh: () => Promise<SessionState>,
    private readonly publish: (state: SessionState) => void,
  ) {}

  async initialize(snapshot: SessionState): Promise<void> {
    if (this.stopped) return;
    if (!this.current || isSnapshotAtLeastAsFresh(this.current, snapshot)) {
      this.current = snapshot;
      this.publish(snapshot);
    }
    await this.flush();
  }

  enqueue(envelope: SequencedSessionEvent): void {
    if (this.stopped) return;
    this.queue.push(envelope);
    if (this.current) void this.flush();
  }

  replace(snapshot: SessionState): void {
    if (this.stopped) return;
    this.replacementVersion += 1;
    this.queue.length = 0;
    this.current = snapshot;
    this.publish(snapshot);
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
      const replacementVersion = this.replacementVersion;
      const next = await reconcileSessionEnvelope(this.current, envelope, this.refresh);
      if (this.stopped) return;
      if (replacementVersion !== this.replacementVersion) continue;
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
  const recovered = isSnapshotAtLeastAsFresh(current, snapshot, envelope.sessionId) ? snapshot : current;
  if (envelope.sessionId === recovered.sessionId && envelope.sequence <= recovered.sequence) return recovered;
  const afterRefresh = applySequencedEvent(recovered, envelope);
  if (afterRefresh) return afterRefresh;

  const finalSnapshot = await refresh();
  return isSnapshotAtLeastAsFresh(recovered, finalSnapshot, envelope.sessionId) ? finalSnapshot : recovered;
}

export function isSnapshotAtLeastAsFresh(
  current: SessionState,
  candidate: SessionState,
  expectedReplacementSessionId?: string,
): boolean {
  if (candidate.sessionId === current.sessionId) return candidate.sequence >= current.sequence;
  return Boolean(expectedReplacementSessionId && candidate.sessionId === expectedReplacementSessionId);
}
