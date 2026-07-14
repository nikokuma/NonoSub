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

The generated media is test/demo material. The repository's MIT license applies to the fixture-building script; use of synthetic voice output remains subject to the applicable OpenAI terms.
