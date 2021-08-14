# av1parser
[![Build Status](https://github.com/yohhoy/av1parser/actions/workflows/rust.yml/badge.svg)](https://github.com/yohhoy/av1parser/actions/workflows/rust.yml)
[![MIT License](http://img.shields.io/badge/license-MIT-blue.svg?style=flat)](LICENSE)

[AOM(Alliance of Open Media)][aom]'s [AV1 video codec bitstream][av1-spec] parser.

The program reads AV1 bitstreams, parses header-level syntax elements, and analyzes the high-level structure of the coded video sequence.

This project is not intended to decode video frames.

[aom]: https://aomedia.org/
[av1-spec]: https://aomedia.org/av1/specification/


## Usage (Example)
Run with maximum verbose output:
```
$ cargo run streams/parkjoy.webm -vvv
...
```

(The semantics of each syntax element are defined in AV1 specification. Enjoy it! :P)


## Details
Supported file formats:
- Raw bitstream (Low overhead bitstream format)
- [IVF format][ivf]
- [WebM format][webm] ("V_AV1" codec)
- [MP4 format][isobmff] ("av01" codec)

[ivf]: https://wiki.multimedia.cx/index.php/IVF
[webm]: https://www.webmproject.org/
[isobmff]: https://en.wikipedia.org/wiki/ISO/IEC_base_media_file_format

Supported OBU types:
- OBU_SEQUENCE_HEADER
- OBU_TEMPORAL_DELIMITER (no payload)
- OBU_FRAME_HEADER
- OBU_FRAME (header part only)
- OBU_TILE_LIST
- OBU_METADATA


## License
MIT License
