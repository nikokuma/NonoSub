<script lang="ts">
  import type { AppSurface } from "$lib/contracts";
  import LessonSurface from "$lib/LessonSurface.svelte";
  import LauncherSurface from "$lib/LauncherSurface.svelte";
  import OverlaySurface from "$lib/OverlaySurface.svelte";
  import ViewerSurface from "$lib/ViewerSurface.svelte";
  import WorkbenchSurface from "$lib/WorkbenchSurface.svelte";

  const requested = typeof window === "undefined" ? "workbench" : new URLSearchParams(window.location.search).get("surface");
  const surface: AppSurface = requested === "viewer" || requested === "overlay" || requested === "lesson" || requested === "launcher" ? requested : "workbench";
</script>

<svelte:head><title>NonoSub — Interactive language subtitles</title></svelte:head>

{#if surface === "viewer"}<ViewerSurface />
{:else if surface === "overlay"}<OverlaySurface />
{:else if surface === "lesson"}<LessonSurface />
{:else if surface === "launcher"}<LauncherSurface />
{:else}<WorkbenchSurface />{/if}
