
## Handling edit lists in an MP4 when transmuxing to fMP4.

We should be able to handle 3 types of edit lists entries:

1. Empty edit at the start. 
   This edit delays the start of the track, i.e. it adds some time at the
   start that is just a blank image or no sounds. Used to align the start of
   audio and video tracks.
2. A negative empty edit at the start (see [Preparing Audio for HTTP Live Streaming](https://developer.apple.com/documentation/http_live_streaming/preparing_audio_for_http_live_streaming)) 
   This is used to skip the first few samples of a track because they are
   initialization data, not actual audio or video.
3. A positive edit over the entire track. 
   This is used because entries in the `CTTS` box (version 0) can only be
   positive, so usually the edit's `media_time` is equal to the first sample's
   CTTS entry's `offset`, making the composition time at the start zero.

How to handle this in fragmented MP4?

- Edit type 1 is not valid in fMP4. 
  We can handle it by using the Track Fragment Base Media Decode Time Box (tfdt).
  Simply have a non-zero value in the first segment.
- Edit type 2 is valid, but only for audio. 
  We can handle it by again using the tftd box, but this time change the
  values in all _other_ tracks.
- Edit type 3 is valid in fMP4, but only for video and only for the reason
  described above, not for other uses. 
  We can handle it by using a type 1 `CTTS` box, which can have negative values.

After these changes, the Edit List Box should be empty. If not, the movie
might not play correctly, the tracks might not be aligned correctly.

