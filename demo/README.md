# NonoSub sample media

`NonoSubTwoSpeakerFixture.mp4` is a roughly 34-second technical fixture for checking Japanese transcription, alternating-speaker diarization, subtitle synchronization, and speaker-label stability.

Nico recorded the two source clips for OpenAI Build Week using two distinct synthetic ChatGPT voices. The checked-in fixture uses only their audio and original NonoSub test-card visuals. It contains no legacy NonoSub source or third-party video footage.

The voices alternate across three turns each. Both voices read the same three-part passage, making this useful for speaker-stability testing but not for the final indirect-refusal teaching demonstration.

Rebuild it on macOS with FFmpeg:

```bash
./scripts/build_two_speaker_fixture.sh \
  /path/to/JpTestFemale.mov \
  /path/to/JpTestMale.mov
```

The generated media is test/demo material. No repository license is granted for the fixture-building script; use of synthetic voice output remains subject to the applicable OpenAI terms.

## Indirect-refusal demo fixture

`NonoSubIndirectRefusalDemo.mp4` is the primary teaching fixture. It uses six separately recorded synthetic Japanese turns—three per speaker—assembled over original NonoSub test-card visuals with no source subtitles baked into the video.

The dialogue is the Build Week teaching script centered on `今日はちょっと……` as a context-dependent indirect refusal. Rebuild it with:

```bash
./scripts/build_indirect_refusal_fixture.sh \
  /path/to/jp-a-01.mov /path/to/jp-b-01.mov \
  /path/to/jp-a-02.mov /path/to/jp-b-02.mov \
  /path/to/jp-a-03.mov /path/to/jp-b-03.mov
```

## English-to-Japanese fixture

`NonoSubEnglishFixture.mp4` wraps Nico's original English voice recording in an original NonoSub test card. It is the reverse-direction acceptance fixture: English source audio is transcribed, then translated into Japanese. No transcript is baked into the video.

Rebuild it with:

```bash
./scripts/build_english_fixture.sh /path/to/EnglishNonoSubTest.m4a
```
