## TODO

- split up crate into `mp4lib` and `mp4streaming`

- instead of `DataRef::read_exact_at()`, implement
  scatter-gather reading via a (scoped?) threadpool.

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
    //   buffer back into Request::data.
  }
}
```

- use the above in pseudostreaming.rs as well

