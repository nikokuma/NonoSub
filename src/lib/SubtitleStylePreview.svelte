<script lang="ts">
  import type { CaptionProcessingMode, SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";
  import { effectiveStyle } from "./preferences";
  import SubtitleStack from "./SubtitleStack.svelte";

  let { style, processingMode }: { style: StyleSettings; processingMode: CaptionProcessingMode } = $props();

  const previewStyle = $derived(effectiveStyle(style, processingMode));
  const speaker: SpeakerProfile = { id: "preview-nono", displayName: "Nono", color: "#ff83bd" };
  const segment: SubtitleSegment = {
    id: "subtitle-style-preview",
    origin: "file",
    startMs: 0,
    endMs: 4_000,
    sourceText: "行きたくないわけじゃないんですけど、今日はちょっと予定があって難しいかもしれません。",
    translationText: "It’s not that I don’t want to go, but I already have plans today, so it might be difficult.",
    speakerId: speaker.id,
    isProvisional: false,
    transcriptionStatus: "complete",
    translationStatus: "complete",
  };
</script>

<div class="preview" aria-label={`Preview of ${style.preset} subtitles`}>
  <div class="scene" aria-hidden="true">
    <div class="light"><i></i><span></span></div>
    <div class="dark"><i></i><span></span></div>
  </div>
  <div class="caption">
    <SubtitleStack
      segments={[segment]}
      speakers={{ [speaker.id]: speaker }}
      style={previewStyle}
      preview
    />
  </div>
  <span class="badge">LIVE PREVIEW · {processingMode === "original_only" ? "ORIGINAL ONLY" : "TRANSLATED"}</span>
</div>

<style>
  .preview{position:relative;height:205px;margin:14px 0 16px;overflow:hidden;border:1px solid #303947;background:#111722;isolation:isolate}
  .scene{position:absolute;inset:0;display:grid;grid-template-columns:1fr 1fr}
  .light{position:relative;overflow:hidden;background:linear-gradient(155deg,#dff2f2 0 38%,#90bbc6 39% 62%,#7f9677 63%)}
  .light:before{content:"";position:absolute;left:8%;top:18%;width:35%;height:45%;background:#f7fbf0;border-radius:50% 50% 12% 12%;filter:blur(7px);opacity:.75}
  .light i{position:absolute;right:10%;bottom:-14%;width:38%;height:72%;background:#425c54;border-radius:48% 48% 8% 8%;filter:blur(2px)}
  .light span{position:absolute;right:18%;top:20%;width:20%;aspect-ratio:1;border-radius:50%;background:#273d3c;box-shadow:0 0 22px #fff8}
  .dark{position:relative;overflow:hidden;background:linear-gradient(145deg,#1c2130,#090c13 58%,#020307)}
  .dark:before{content:"";position:absolute;left:12%;bottom:0;width:64%;height:65%;background:linear-gradient(90deg,#241b2c,#101827);clip-path:polygon(7% 100%,18% 22%,43% 37%,53% 7%,74% 27%,91% 100%);filter:blur(2px)}
  .dark i{position:absolute;right:10%;top:16%;width:29%;height:2px;background:#ff74b7;box-shadow:0 0 16px #ff74b7}
  .dark span{position:absolute;right:8%;top:25%;width:34%;height:40%;border:1px solid #5ce7df55;background:#0d273044}
  .caption{position:absolute;inset:28px 28px 12px;display:grid;place-items:center;z-index:2;pointer-events:none}
  .badge{position:absolute;left:9px;top:7px;z-index:3;color:#d9e3e9;background:#080c12b8;border:1px solid #ffffff1c;padding:4px 6px;font-size:7px;font-weight:800;letter-spacing:.13em}
  @media(max-width:760px){.preview{height:180px}.caption{inset:25px 12px 8px}}
</style>
