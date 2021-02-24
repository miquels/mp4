
## Chromecast subtitle support:

[Robin Davies](https://stackoverflow.com/users/1232937/robin-davies) in
https://stackoverflow.com/questions/54739442/ :

WebVTT support is partial.

Supported:

- `<i></i>`, and `<b></b>`
- Positioning and alignment attributes such as 00:06.790 --> 00:07.830 position:10%,line-left align:left size:35% (perhaps a subset)
- `&lt;&gt;&amp;` entities

Not supported:

- CSS of any kind.
- `<c></c>` in any variant (e.g. not `<c.red>`)
- `<ruby`
- `<v>`

And probably not chapters, since I can't imagine what they would be used for if they were implement.

The presence of a Byte-Order Mark on the first line causes the entire file to be rejected. (That's probably not incorrect. But it is perilous for Windows developers).

All linefeeds and "\n" are hard line-breaks.

## Segmented WebVTT example (does not work on chromecast?)

```
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-TARGETDURATION:7310
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:7309.400000,
https://undertekst.nrk.no/prod/MSUB19/12/MSUB19121216AW/MIX/MSUB19121216AW-v2.vtt
#EXT-X-DISCONTINUITY
#EXTINF:5891.920000,
https://undertekst.nrk.no/prod/MSUB19/12/MSUB19121216BW/TTV/MSUB19121216BW-v2.vtt
#EXT-X-ENDLIST
```

