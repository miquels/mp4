# MP4

This repository contains 3 rust crates:

## [`mp4lib`](mp4lib/)

A library to read, modify, and write MP4 files. It can also rewrite MP4 files
on-the-fly for use in streaming servers.

### HTML5 pseudo streaming

To be used with a standard HTML5 `<video>` element.

- only include selected track(s) (useful for audio switching)
- put the MovieFragmentBox at the front of the file (faster loading)
- re-interleave the tracks (prevents stuttering)

### HLS

Serves an MP4 file as a HLS VOD resource, with m3u manifest, and CMAF segments
(not done yet)


## [`mp4cli`](mp4cli/)

A cli tool called `mp4` that can

- show information about mp4 files
- edit/rewrite mp4 files


## [`mpserver`](mp4cli/)

HTTP server:

- serves MP4 files, optimized for streaming, can select tracks via query params
- serves embedded subtitles as .vtt resource
- TODO: serves HLS

