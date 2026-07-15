#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 6 || $# -gt 7 ]]; then
  echo "Usage: $0 A1.mov B1.mov A2.mov B2.mov A3.mov B3.mov [OUTPUT.mp4]" >&2
  exit 2
fi

output=${7:-demo/NonoSubIndirectRefusalDemo.mp4}
for source in "${@:1:6}"; do
  if [[ ! -f "$source" ]]; then
    echo "Missing source recording: $source" >&2
    exit 2
  fi
done

mkdir -p "$(dirname "$output")"

ffmpeg -hide_banner -loglevel warning -y \
  -i "$1" -i "$2" -i "$3" -i "$4" -i "$5" -i "$6" \
  -f lavfi -i "color=c=0x090b12:s=1280x720:r=30:d=26" \
  -filter_complex "
    [0:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a1];
    [1:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[b1];
    [2:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a2];
    [3:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[b2];
    [4:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a3];
    [5:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[b3];
    anullsrc=r=48000:cl=stereo,atrim=duration=2.50[lead];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.90[g1];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.90[g2];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.90[g3];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.90[g4];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.90[g5];
    anullsrc=r=48000:cl=stereo,atrim=duration=2.50[tail];
    [lead][a1][g1][b1][g2][a2][g3][b2][g4][a3][g5][b3][tail]
      concat=n=13:v=0:a=1,loudnorm=I=-16:TP=-1.5:LRA=11[audio];
    [6:v]
      drawbox=x=0:y=0:w=1280:h=720:color=0x090b12:t=fill,
      drawbox=x=52:y=48:w=1176:h=624:color=0x10151f:t=fill,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='NonoSub':fontcolor=0xff70b7:fontsize=42:x=84:y=78,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='INDIRECT REFUSAL · ORIGINAL BUILD WEEK FIXTURE':fontcolor=0x7d8795:fontsize=17:x=84:y=136,
      drawbox=x=84:y=214:w=500:h=328:color=0x25182b:t=fill,
      drawbox=x=696:y=214:w=500:h=328:color=0x112b2d:t=fill,
      drawbox=x=84:y=214:w=500:h=328:color=0xff70b7@0.34:t=14:enable='between(t,2.50,5.54)+between(t,8.34,10.78)+between(t,15.64,18.33)',
      drawbox=x=696:y=214:w=500:h=328:color=0x6ce1d9@0.34:t=14:enable='between(t,6.44,7.44)+between(t,11.68,14.75)+between(t,19.23,21.40)',
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='SPEAKER A':fontcolor=0xffffff:fontsize=34:x=244:y=345,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='SPEAKER B':fontcolor=0xffffff:fontsize=34:x=856:y=345,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Speaking':fontcolor=0xffafd5:fontsize=21:x=284:y=407:enable='between(t,2.50,5.54)+between(t,8.34,10.78)+between(t,15.64,18.33)',
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Speaking':fontcolor=0xa0f2e9:fontsize=21:x=896:y=407:enable='between(t,6.44,7.44)+between(t,11.68,14.75)+between(t,19.23,21.40)',
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Synthetic Japanese speech · no source subtitles baked in':fontcolor=0x707987:fontsize=18:x=(w-text_w)/2:y=601[video]
  " \
  -map "[video]" -map "[audio]" \
  -c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p \
  -c:a aac -b:a 192k -ar 48000 -movflags +faststart -shortest \
  "$output"

echo "Created $output"
