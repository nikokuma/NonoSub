import type { LessonMessage } from "./contracts";

export const MAX_LESSON_THREAD_KEYS = 32;
export const MAX_LESSON_MESSAGES_PER_THREAD = 32;

export interface LessonThreadStore {
  threads: Record<string, LessonMessage[]>;
  order: string[];
}

export function capLessonMessages(messages: LessonMessage[]): LessonMessage[] {
  return messages.slice(-MAX_LESSON_MESSAGES_PER_THREAD);
}

export function rememberLessonThread(
  store: LessonThreadStore,
  key: string,
  messages: LessonMessage[],
): LessonThreadStore {
  if (!key) return store;
  const threads = { ...store.threads, [key]: capLessonMessages(messages) };
  const order = [...store.order.filter((candidate) => candidate !== key), key];
  while (order.length > MAX_LESSON_THREAD_KEYS) {
    const evicted = order.shift();
    if (evicted) delete threads[evicted];
  }
  return { threads, order };
}

export function recallLessonThread(store: LessonThreadStore, key: string): {
  store: LessonThreadStore;
  messages: LessonMessage[];
} {
  const messages = store.threads[key];
  if (!messages) return { store, messages: [] };
  return {
    store: {
      threads: store.threads,
      order: [...store.order.filter((candidate) => candidate !== key), key],
    },
    messages: [...messages],
  };
}
