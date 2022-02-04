## TODO

In no particular order.

- better async support, preferably both sync and async.

- clean up subtitle support.

- pseudo-streaming and HLS feature parity:
  - embed external subtitles as MP4 track (apple devices understand this, good for airplay)
  - serve internal subtitles as external (needs optimization / caching)

- maybe split up crate into `mp4lib` and `mp4streaming`?
  probably the current split with `streaming` in a separate module
  is good enough.

- investigate how hard it is to add MKV support (for reading at least)

- investigate interfacing with `gstreamer`. it would be very cool to be able
  to transmux MKV or even transcode other formats.

## OPTIMIZATIONS

we use `mmap` to read the MOOV box, and normal reads to read the
media data from the MDAT box. Except in `streaming/pseudo.rs` where
a different mmap strategy is followed.

But perhaps instead of `DataRef::read_exact_at()`, we should implement
scatter-gather reading via a (scoped?) threadpool. Something like:

```
struct Request {
  file:   Arc<fs::File>,
  offset: u64,
  data:   &mut [u8],
}

impl DataRef {
  fn read_requests(&self, &[Request]) -> io::Result<()> {
    // - sort requests by offset
    // - then find some heuristic to decide if we should
    //   issue seperate read requests, or just issue one
    //   big request into a buffer, then copy parts of that
    //   buffer back into Request::data, etc.
  }
}
```

