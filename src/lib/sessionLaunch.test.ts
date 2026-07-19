import { describe, expect, it } from "vitest";
import { validateVideoPath } from "./sessionLaunch";

describe("session launcher", () => {
  it.each(["movie.mp4", "/Users/nico/clip.MOV"])("accepts %s", (path) => {
    expect(validateVideoPath(path)).toBeUndefined();
  });

  it.each(["", "movie.mkv", "video.mp4.txt"])("rejects %s", (path) => {
    expect(validateVideoPath(path)).toBeTruthy();
  });
});
