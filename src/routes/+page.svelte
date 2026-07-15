<script lang="ts">
  import type { AppSurface } from "$lib/contracts";
  import LessonSurface from "$lib/LessonSurface.svelte";
  import OverlaySurface from "$lib/OverlaySurface.svelte";
  import ViewerSurface from "$lib/ViewerSurface.svelte";
  import WorkbenchSurface from "$lib/WorkbenchSurface.svelte";

  const requested = typeof window === "undefined" ? "workbench" : new URLSearchParams(window.location.search).get("surface");
  const surface: AppSurface = requested === "viewer" || requested === "overlay" || requested === "lesson" ? requested : "workbench";
</script>

<svelte:head><title>NonoSub — Interactive language subtitles</title></svelte:head>

{#if surface === "viewer"}<ViewerSurface />
{:else if surface === "overlay"}<OverlaySurface />
{:else if surface === "lesson"}<LessonSurface />
{:else}<WorkbenchSurface />{/if}
