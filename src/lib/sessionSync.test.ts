import { describe, expect, it, vi } from "vitest";
import { EMPTY_SESSION } from "./contracts";
import type { SequencedSessionEvent, SessionState } from "./contracts";
import { SessionEventCoordinator, isSnapshotAtLeastAsFresh, reconcileSessionEnvelope } from "./sessionSync";

function snapshot(sessionId: string, sequence: number, phase: SessionState["phase"] = "preparing"): SessionState {
  return { ...structuredClone(EMPTY_SESSION), sessionId, sequence, phase };
}

function phaseEvent(sessionId: string, sequence: number, phase: SessionState["phase"]): SequencedSessionEvent {
  return { sessionId, sequence, event: { type: "phase_changed", phase } };
}

describe("multi-window session synchronization", () => {
  it("applies an event queued before the initial snapshot resolves", async () => {
    const published: SessionState[] = [];
    const coordinator = new SessionEventCoordinator(
      async () => snapshot("session-1", 1, "transcribing"),
      (state) => published.push(state),
    );
    coordinator.enqueue(phaseEvent("session-1", 1, "transcribing"));
    await coordinator.initialize(snapshot("session-1", 0));

    expect(published.at(-1)?.sequence).toBe(1);
    expect(published.at(-1)?.phase).toBe("transcribing");
  });

  it("does not apply a queued event already represented by the snapshot twice", async () => {
    const published: SessionState[] = [];
    const coordinator = new SessionEventCoordinator(
      async () => snapshot("session-1", 1, "transcribing"),
      (state) => published.push(state),
    );
    coordinator.enqueue(phaseEvent("session-1", 1, "transcribing"));
    await coordinator.initialize(snapshot("session-1", 1, "transcribing"));

    expect(published).toHaveLength(1);
    expect(published[0].sequence).toBe(1);
  });

  it("refreshes one canonical snapshot when an event gap is detected", async () => {
    const refresh = vi.fn(async () => snapshot("session-1", 3, "ready"));
    const result = await reconcileSessionEnvelope(
      snapshot("session-1", 1, "transcribing"),
      phaseEvent("session-1", 3, "ready"),
      refresh,
    );

    expect(refresh).toHaveBeenCalledOnce();
    expect(result.sequence).toBe(3);
    expect(result.phase).toBe("ready");
  });

  it("converges to a replacement session instead of applying it to the old one", async () => {
    const replacement = snapshot("session-2", 1, "preparing");
    const result = await reconcileSessionEnvelope(
      snapshot("session-1", 8, "complete"),
      {
        sessionId: "session-2",
        sequence: 1,
        event: {
          type: "session_reset",
          mode: "file",
          languages: { source: "ja", target: "en", explanation: "en" },
          processingMode: "translated",
        },
      },
      async () => replacement,
    );

    expect(result).toEqual(replacement);
  });

  it("serializes concurrent gap recovery and ignores late older envelopes", async () => {
    const published: SessionState[] = [];
    const refresh = vi.fn(async () => snapshot("session-1", 3, "ready"));
    const coordinator = new SessionEventCoordinator(refresh, (state) => published.push(state));
    await coordinator.initialize(snapshot("session-1", 0));
    coordinator.enqueue(phaseEvent("session-1", 3, "ready"));
    coordinator.enqueue(phaseEvent("session-1", 1, "transcribing"));
    await coordinator.flush();

    expect(refresh).toHaveBeenCalledOnce();
    expect(published.at(-1)?.sequence).toBe(3);
    expect(published.at(-1)?.phase).toBe("ready");
  });

  it("replaces visible state authoritatively when a session ends", async () => {
    const published: SessionState[] = [];
    const coordinator = new SessionEventCoordinator(
      async () => snapshot("session-1", 5, "playing"),
      (state) => published.push(state),
    );
    await coordinator.initialize(snapshot("session-1", 5, "playing"));
    coordinator.enqueue(phaseEvent("session-1", 6, "complete"));
    coordinator.replace(snapshot("idle", 0, "idle"));
    await coordinator.flush();

    expect(published.at(-1)?.sessionId).toBe("idle");
    expect(published.at(-1)?.phase).toBe("idle");
  });

  it("rejects a late snapshot that would move the same session backward", () => {
    expect(isSnapshotAtLeastAsFresh(snapshot("session-1", 8), snapshot("session-1", 7))).toBe(false);
    expect(isSnapshotAtLeastAsFresh(snapshot("session-1", 8), snapshot("session-1", 8))).toBe(true);
  });

  it("only accepts a replacement session when it matches the triggering envelope", () => {
    expect(isSnapshotAtLeastAsFresh(snapshot("session-1", 8), snapshot("session-2", 1), "session-2")).toBe(true);
    expect(isSnapshotAtLeastAsFresh(snapshot("session-1", 8), snapshot("session-stale", 99), "session-2")).toBe(false);
  });

  it("keeps newer state when the final recovery snapshot is stale", async () => {
    const refresh = vi.fn()
      .mockResolvedValueOnce(snapshot("session-1", 4, "ready"))
      .mockResolvedValueOnce(snapshot("session-1", 2, "transcribing"));
    const result = await reconcileSessionEnvelope(
      snapshot("session-1", 3, "transcribing"),
      phaseEvent("session-1", 6, "complete"),
      refresh,
    );
    expect(result.sequence).toBe(4);
    expect(result.phase).toBe("ready");
  });
});
