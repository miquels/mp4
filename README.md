# MP4

This repository contains 3 rust crates:

## [`mp4lib`](mp4lib/)

A library to read, modify, and write MP4 files. It can also rewrite MP4 files
on-the-fly for use in streaming servers.

- remap MP4 file to have MovieBox at the front
- interleave tracks
- split tracks into media segments
- generate media initialization section
- generate HLS manifest
- extract subtitles and rewrite from TX3G to WebVTT or SRT format.

## [`mp4cli`](mp4cli/)

A cli tool called `mp4` that can

- show information about mp4 files ("mediainfo", "boxes")
- edit/rewrite mp4 files (MOOV at front, re-interleaving, enabling/disabling tracks)
- extract subtitles.

## [`mp4server`](mp4server/)

HTTP server:

- serves MP4 files, optimized for streaming, can select tracks via query params
- serves embedded subtitles as .vtt resource
- serves MP4 files as HLS resources.

### Pseudo-streaming.

To be used with a standard HTML5 `<video>` element.

- only include selected track(s) (useful for audio switching)
- put the MovieFragmentBox at the front of the file (faster loading)
- re-interleave the tracks (prevents stuttering)
- serve embedded TX3G subtitle tracks as WebVTT tracks

```
https://your.server/path/file.mp4?track=1&track=3
```
This serves `file.mp4` remapped to have the MovieBox at the start, and to only
contain tracks `1` and `3` which are interleaved with a 500ms interval.

Useful when you are building a video player and you don't want the overhead of
HLS streaming, but you want to be able to switch audio tracks, and show
subtitles.

### HLS streaming

To be used with apple devices, Shaka Player, Video.js etc. If you serve
a standard MP4 file, you'll only get video, the first audio track, and
no subtitles, while if you serve a HLS playlist, you can pick and
choose using the player's UI.

```
https://your.server/path/file.mp4/main.m3u8
```
This serves the main HLS playlist. In it, there references to playlists for individual
video/audio/subtitle tracks. Those refer to media segments.

- track playlist: `file.mp4/media.TRACK_ID.m3u8`
- media initialization section: `file.mp4/init.TRACK_ID.mp4`
- video segments: `file.mp4/v/c.TRACK_ID.FROMSAMPLE-TOSAMPLE.mp4`
- audio segments: `file.mp4/a/c.TRACK_ID.FROMSAMPLE-TOSAMPLE.m4a`
- subtitle segments: `file.mp4/a/c.TRACK_ID.FROMSAMPLE-TOSAMPLE.m4a`

# Copyright and License

Copyright (C) Miquel van Smoorenburg.

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](Licenses/LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](Licenses/LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
