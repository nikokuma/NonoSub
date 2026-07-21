# Judge instructions

NonoSub is verified on Apple Silicon macOS. File mode is the guaranteed submission path; Live Captions requires macOS 14 or later and permission for Screen & System Audio Recording.

## Fastest test

1. Launch NonoSub. First launch opens the Wired-style workbench; later launches remain in the menu bar.
2. Read the privacy disclosure, enter an OpenAI API key, and validate model access.
3. Keep Source on **Auto-detect**, Subtitles on **English**, and Nono explains in **English**.
4. Choose **Open MP4 or MOV** and select `demo/NonoSubIndirectRefusalDemo.mp4` from the repository.
5. Wait for translated coverage, then watch the six-turn conversation in the minimal viewer.
6. Right-click `今日はちょっと……`, choose **What is omitted?**, and ask why the incomplete phrase functions as a polite refusal.
7. Return to the workbench to rename a speaker, change a speaker color, or try the six subtitle presets.

For the reverse-direction path, set Subtitles to **Japanese** and open `demo/NonoSubEnglishFixture.mp4`. Changing a file target reuses its transcript and does not retranscribe the media.

## Live Captions

Choose **Start Live Captions…** from the menu bar. NonoSub displays a visible list of shareable applications, windows, and displays; select the browser or media application there and press **Start Captions**. NonoSub captures only the selected system audio, excludes its own process audio, and does not write PCM to disk. Live mode intentionally uses one `Live Audio` identity rather than claiming reliable realtime diarization.

Settings offers two next-session engines. **Realtime — Fast** is the default and uses `gpt-realtime-translate`. **Transcript-Locked — Accurate (Experimental)** finalizes the source with `gpt-realtime-whisper`, then translates that immutable clause with `gpt-5.6-luna`. Accurate mode holds the previous readable caption until its validated pair is ready; failures keep a clickable source-only caption rather than falling back to another engine.

If permission is denied, enable NonoSub under **System Settings → Privacy & Security → Screen & System Audio Recording**, then relaunch. File mode remains available regardless of live-capture permission.

## Unsigned artifact

The Build Week artifact is ad-hoc signed and not notarized. Building from source is the preferred verification path. If macOS quarantines a downloaded copy and you trust this public repository, move the app to `/Applications`, then run:

```bash
xattr -dr com.apple.quarantine /Applications/NonoSub.app
open /Applications/NonoSub.app
```

## Source build

Requirements: Apple Silicon macOS 14+, Node.js 20+, pnpm 10+, Rust 1.97.1 (pinned by `rust-toolchain.toml`), and Xcode command-line tools.

```bash
pnpm install --frozen-lockfile
pnpm verify
pnpm tauri dev
```

The API key is sent directly from onboarding to Rust and stored in the OS credential vault. It is never returned to a webview. See [Privacy](PRIVACY.md), [Architecture](ARCHITECTURE.md), and [Third-party notices](../THIRD_PARTY_NOTICES.md).
