import { afterEach, describe, expect, it, vi } from "vitest";
import { maintainSubscription } from "./runtime";

afterEach(() => vi.useRealTimers());

describe("runtime subscription lifecycle", () => {
  it("retries initial failures and clears the recoverable status after connection", async () => {
    vi.useFakeTimers();
    const unlisten = vi.fn();
    const subscribe = vi.fn()
      .mockRejectedValueOnce(new Error("window event bridge unavailable"))
      .mockResolvedValueOnce(unlisten);
    const statuses: string[] = [];
    const stop = maintainSubscription(subscribe, (message) => statuses.push(message));
    await vi.waitFor(() => expect(subscribe).toHaveBeenCalledTimes(1));
    await vi.advanceTimersByTimeAsync(500);
    await vi.waitFor(() => expect(subscribe).toHaveBeenCalledTimes(2));
    expect(statuses[0]).toContain("retrying");
    expect(statuses.at(-1)).toBe("");
    stop();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("immediately disposes a listener that resolves after unmount", async () => {
    let resolve!: (unlisten: () => void) => void;
    const unlisten = vi.fn();
    const pending = new Promise<() => void>((done) => resolve = done);
    const stop = maintainSubscription(() => pending);
    stop();
    resolve(unlisten);
    await pending;
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledOnce();
  });
});
