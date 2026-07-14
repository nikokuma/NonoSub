#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "Usage: $0 FEMALE.mov MALE.mov [OUTPUT.mp4]" >&2
  exit 2
fi

female_source=$1
male_source=$2
output=${3:-demo/NonoSubTwoSpeakerFixture.mp4}

for source in "$female_source" "$male_source"; do
  if [[ ! -f "$source" ]]; then
    echo "Missing source recording: $source" >&2
    exit 2
  fi
done

mkdir -p "$(dirname "$output")"

ffmpeg -hide_banner -loglevel warning -y \
  -i "$female_source" \
  -i "$male_source" \
  -f lavfi -i "color=c=0x0d0b17:s=1280x720:r=30:d=34" \
  -filter_complex "
    [0:a]atrim=start=0.25:end=5.15,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[f1];
    [1:a]atrim=start=0.20:end=5.45,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[m1];
    [0:a]atrim=start=5.45:end=9.95,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[f2];
    [1:a]atrim=start=5.95:end=9.95,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[m2];
    [0:a]atrim=start=10.30:end=17.05,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[f3];
    [1:a]atrim=start=10.20:end=15.55,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[m3];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.60[g1];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.60[g2];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.60[g3];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.60[g4];
    anullsrc=r=48000:cl=stereo,atrim=duration=0.60[g5];
    [f1][g1][m1][g2][f2][g3][m2][g4][f3][g5][m3]concat=n=11:v=0:a=1,loudnorm=I=-16:TP=-1.5:LRA=11[audio];
    [2:v]
      drawbox=x=64:y=64:w=1152:h=592:color=0x171326:t=fill,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='NonoSub':fontcolor=0xff72b6:fontsize=38:x=96:y=92,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='TWO-SPEAKER STABILITY FIXTURE':fontcolor=0x8f899f:fontsize=18:x=96:y=143,
      drawbox=x=96:y=208:w=500:h=330:color=0x241b35:t=fill,
      drawbox=x=684:y=208:w=500:h=330:color=0x152b30:t=fill,
      drawbox=x=96:y=208:w=500:h=330:color=0xff72b6@0.35:t=16:enable='between(t,0,4.9)+between(t,11.35,15.85)+between(t,21.05,27.8)',
      drawbox=x=684:y=208:w=500:h=330:color=0x79e9cb@0.35:t=16:enable='between(t,5.5,10.75)+between(t,16.45,20.45)+between(t,28.4,33.75)',
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='FEMALE VOICE':fontcolor=0xffffff:fontsize=30:x=236:y=330,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial Bold.ttf':text='MALE VOICE':fontcolor=0xffffff:fontsize=30:x=855:y=330,
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Speaking':fontcolor=0xffa8d2:fontsize=22:x=292:y=390:enable='between(t,0,4.9)+between(t,11.35,15.85)+between(t,21.05,27.8)',
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Speaking':fontcolor=0x9df4dc:fontsize=22:x=903:y=390:enable='between(t,5.5,10.75)+between(t,16.45,20.45)+between(t,28.4,33.75)',
      drawtext=fontfile='/System/Library/Fonts/Supplemental/Arial.ttf':text='Synthetic Japanese speech - recorded for Build Week':fontcolor=0x777181:fontsize=18:x=(w-text_w)/2:y=596[video]
  " \
  -map "[video]" -map "[audio]" \
  -c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p \
  -c:a aac -b:a 192k -ar 48000 -movflags +faststart -shortest \
  "$output"

echo "Created $output"
