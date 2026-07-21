# GPT-5.6 Luna live-translation benchmark

Date: July 21, 2026

Purpose: choose the fixed reasoning effort for NonoSub's experimental **Transcript-Locked — Accurate** live engine. The production user cannot change this setting.

## Method

- Model: `gpt-5.6-luna`
- API: Responses API with `stream: true`, `store: false`, and a 256-token output ceiling
- Cases: 24 authored clauses, split evenly between Japanese→English and English→Japanese
- Repetitions: two per case at `none` and `low` (96 requests per matrix)
- Coverage: Arabic digits, dates, prices, names, negation, uncertainty, indirect refusal, omitted subjects, idioms, politeness, long clauses, and transcript text resembling prompt injection
- Pricing used for estimates: $1.00 per million input tokens and $6.00 per million output tokens, as published for GPT-5.6 Luna on July 21, 2026

The first matrix exposed a weakness in the numeric prompt. A second 96-request matrix was run after adding an explicit `required_decimal_sequences` list. Diagnostic output contained only authored benchmark translations; no user transcript, API key, or request ID was logged.

## Final matrix

| Effort | Requests | Hard failures | Entity failures | Semantic accuracy | Leakage | First-delta p95 | Completion p95 | Tokens in/out | Est. cost |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `none` | 48 | 2 numeric-lock failures | 0 among completed results | 95.83% | 0 | 3,688 ms | 4,697 ms | 7,600 / 912 | $0.013072 |
| `low` | 48 | 0 | 0 | 100% | 0 | 3,504 ms | 3,649 ms | 7,946 / 2,121 | $0.020672 |

Three initially failed semantic checks were corrected before scoring because the rubric omitted valid equivalents: “a bit/kind of” for hesitant `ちょっと`, `助ける` for “help,” and `申し訳ありません` for a formal apology. The source cases and acceptance categories did not change.

## Decision

`low` is the only eligible effort:

- 100% entity and Arabic-digit preservation;
- 100% corrected semantic-rubric accuracy;
- no instruction leakage or unexplained additions;
- completion p95 below five seconds.

NonoSub therefore fixes Transcript-Locked translation at `low`. A failed request retries once at `low`. Realtime — Fast remains the default engine and rollback path.

The two matrices used 192 paid requests in total and an estimated $0.057341. The ignored Rust benchmark remains in `src-tauri/src/luna_benchmark.rs` for an explicit, opt-in rerun.
