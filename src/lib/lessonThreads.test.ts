import { describe, expect, it } from "vitest";
import type { LessonMessage } from "./contracts";
import {
  capLessonMessages,
  MAX_LESSON_MESSAGES_PER_THREAD,
  MAX_LESSON_THREAD_KEYS,
  recallLessonThread,
  rememberLessonThread,
  type LessonThreadStore,
} from "./lessonThreads";

function message(index: number): LessonMessage {
  return { id: String(index), role: index % 2 ? "assistant" : "user", text: `message-${index}` };
}

describe("lesson thread bounds", () => {
  it("keeps only the newest messages in a thread", () => {
    const messages = Array.from({ length: 50 }, (_, index) => message(index));
    const bounded = capLessonMessages(messages);
    expect(bounded).toHaveLength(MAX_LESSON_MESSAGES_PER_THREAD);
    expect(bounded[0].id).toBe("18");
    expect(bounded.at(-1)?.id).toBe("49");
  });

  it("evicts the least recently used inactive thread", () => {
    let store: LessonThreadStore = { threads: {}, order: [] };
    for (let index = 0; index <= MAX_LESSON_THREAD_KEYS; index += 1) {
      store = rememberLessonThread(store, `thread-${index}`, [message(index)]);
    }
    expect(Object.keys(store.threads)).toHaveLength(MAX_LESSON_THREAD_KEYS);
    expect(store.threads["thread-0"]).toBeUndefined();
    expect(store.threads[`thread-${MAX_LESSON_THREAD_KEYS}`]).toBeDefined();
  });

  it("touches a recalled thread so another thread is evicted first", () => {
    let store: LessonThreadStore = { threads: {}, order: [] };
    for (let index = 0; index < MAX_LESSON_THREAD_KEYS; index += 1) {
      store = rememberLessonThread(store, `thread-${index}`, [message(index)]);
    }
    store = recallLessonThread(store, "thread-0").store;
    store = rememberLessonThread(store, "new-thread", [message(99)]);
    expect(store.threads["thread-0"]).toBeDefined();
    expect(store.threads["thread-1"]).toBeUndefined();
  });
});
