# Privacy and data flow

## First-run disclosure

> Your video remains on this device. NonoSub extracts audio locally and sends temporary audio chunks to OpenAI for transcription. In Live Captions, audio from the app, window, or display you choose in NonoSub's source chooser is streamed to OpenAI and never saved. The chooser is populated from Apple's ScreenCaptureKit shareable-content list. Transcript context and your questions are sent to GPT‑5.6 for translation and teaching. OpenAI API data is not used for training by default. Standard Responses requests may be retained for abuse monitoring for up to 30 days even with `store:false`. NonoSub has no account, analytics, subscription, hosted proxy, or cloud session database.

## Data inventory

| Data | Destination | Persistence |
| --- | --- | --- |
| Selected video | Local Tauri asset protocol only | Existing user file |
| Extracted WAV chunks | OpenAI transcription API | Local temp directory for current session; transcription endpoint follows OpenAI API controls |
| Selected live system audio | OpenAI realtime translation | Memory only until each approximately 100 ms batch is transmitted; never written to disk |
| Transcript context | GPT‑5.6 Responses API | Current app memory; request uses `store:false` |
| Tutor question/thread | GPT‑5.6 Responses API | Current app memory; request uses `store:false` |
| OpenAI API key | OS credential vault | Until user removes it |
| API-key configured marker | Local app settings | Boolean presence only; contains no key material |
| Subtitle/language/learner preferences | Local webview storage | Until app data is cleared |

Debug builds may optionally receive `OPENAI_API_KEY` from their parent process to avoid repeated macOS prompts while an ad-hoc binary is changing. That fallback is compiled out of release builds, is consumed only by Rust, and still never crosses into a webview.

## Logging rule

NonoSub must never log API keys, Authorization headers, full request bodies, transcript text, tutor questions, audio bytes, or data-URL speaker references. User-facing errors are classified and sanitized before crossing the Tauri boundary.
