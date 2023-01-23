## Safari, MP4 files, and partial requests.

This is just documenting some weird inefficient behaviour that I've
seen from Safari.

When streaming an MP4 file, browsers tend to make partial requests.
Requesting a `Range: bytes=0-` at the start and `Range: bytes=6347364-`
after a seek. The connection is then simply kept open and the video streams.

Safari seems to make many smaller range requests, but upon checking it
turns out that it does in fact requests those large ranges, and
then simply after downloading 3MB, closes the connection hard, and
starts another range request to continue where it left off!

This is quite inconvenient for our pseudo streaming handler, because
it tends to do quite a bit of work that is then buffered in the
output buffer and discarded.

A possible way to guard against this is to limit the size of the
range requests to, say, 2 MB - the HTTP spec does allow you to
return less in response to a range request than requested. We could
do that by adding a `limit_range_response() -> bool` method to the
`HttpFile` trait, then if the User-Agent contains 'Safari' and
`limit_range_response()` returns true (or a size?) we limit the
range response size.

