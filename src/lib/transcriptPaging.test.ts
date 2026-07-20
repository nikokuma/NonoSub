import { describe, expect, it } from "vitest";
import { earlierTranscriptCount, TRANSCRIPT_PAGE_SIZE, visibleTranscriptPage } from "./transcriptPaging";

describe("transcript paging", () => {
  it("shows the newest two hundred lines initially", () => {
    const transcript = Array.from({ length: 1_000 }, (_, index) => index);
    expect(visibleTranscriptPage(transcript, TRANSCRIPT_PAGE_SIZE)).toEqual(transcript.slice(800));
    expect(earlierTranscriptCount(transcript.length, TRANSCRIPT_PAGE_SIZE)).toBe(800);
  });

  it("loads history in bounded two-hundred-line batches", () => {
    const transcript = Array.from({ length: 1_000 }, (_, index) => index);
    expect(visibleTranscriptPage(transcript, 400)).toEqual(transcript.slice(600));
    expect(earlierTranscriptCount(transcript.length, 400)).toBe(600);
  });
});
