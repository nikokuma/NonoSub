#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 INPUT_AUDIO [OUTPUT.mp4]" >&2
  exit 2
fi

input=$1
output=${2:-demo/NonoSubEnglishFixture.mp4}

if [[ ! -f "$input" ]]; then
  echo "Missing source recording: $input" >&2
  exit 2
fi

duration=$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$input")
mkdir -p "$(dirname "$output")"

ffmpeg -hide_banner -loglevel warning -y \
  -i "$input" \
  -f lavfi -i "color=c=0x090b12:s=1280x720:r=30:d=$duration" \
  -filter_complex "
    [0:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,
      loudnorm=I=-16:TP=-1.5:LRA=11[audio];
    [1:v]
      drawbox=x=0:y=0:w=1280:h=720:color=0x090b12:t=fill,
      drawbox=x=52:y=48:w=1176:h=624:color=0x10151f:t=fill,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='NonoSub':fontcolor=0xff70b7:fontsize=42:x=84:y=78,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='ANY-LANGUAGE FILE MODE · ORIGINAL BUILD WEEK FIXTURE':fontcolor=0x7d8795:fontsize=17:x=84:y=136,
      drawbox=x=160:y=225:w=960:h=270:color=0x111c25:t=fill,
      drawbox=x=160:y=225:w=960:h=270:color=0x6ce1d9@0.35:t=12,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='ENGLISH SOURCE':fontcolor=0xffffff:fontsize=48:x=(w-text_w)/2:y=302,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Translate this recording into Japanese':fontcolor=0xa0f2e9:fontsize=27:x=(w-text_w)/2:y=378,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Original voice recording · no source subtitles baked in':fontcolor=0x707987:fontsize=18:x=(w-text_w)/2:y=601[video]
  " \
  -map "[video]" -map "[audio]" \
  -c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p \
  -c:a aac -b:a 192k -ar 48000 -movflags +faststart -shortest \
  "$output"

echo "Created $output"
