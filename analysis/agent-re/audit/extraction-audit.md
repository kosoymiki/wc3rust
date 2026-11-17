# Extraction audit

- archive: `/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq`
- extracted: `analysis/war3_full3`
- entries: **10523**
- verdict: **10523 ok** / 0 read_fail / 0 missing / 0 size_mismatch / 0 sha_mismatch
- bytes: mpq=710907954 disk=710907954 (0 zero-length, 0 zero-content)
- timing: open=335.2 ms, index=71.6 ms, full re-read+sha256=486.22 s
- files on disk not in listfile: 0

## Codecs actually used

| codec | files |
|---|---:|
| stored | 4122 |
| zlib | 3655 |
| stored+zlib | 61 |
| stored+adpcm-mono+huffman | 25 |
| zlib+adpcm-mono+huffman | 2532 |
| stored+zlib+adpcm-mono+huffman | 93 |
| zlib+adpcm-stereo+huffman | 35 |

## Extensions

| ext | files | bytes |
|---|---:|---:|
| ai | 91 | 409319 |
| blp | 3935 | 148430928 |
| dls | 7 | 6963642 |
| fdf | 71 | 551616 |
| ifl | 3 | 10170 |
| ini | 1 | 357 |
| j | 3 | 432697 |
| mdx | 2048 | 67376096 |
| mid | 15 | 208524 |
| mp3 | 1140 | 138644930 |
| mpq | 16 | 32647307 |
| pld | 27 | 138063 |
| slk | 33 | 3662065 |
| tga | 319 | 9169496 |
| toc | 1 | 2355 |
| ttf | 6 | 18374120 |
| txt | 67 | 1299895 |
| w3m | 44 | 9980914 |
| wav | 2696 | 272605460 |

No failures.
