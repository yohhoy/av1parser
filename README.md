# av1parser
[![Build Status](https://travis-ci.org/yohhoy/av1parser.svg?branch=master)](https://travis-ci.org/yohhoy/av1parser)
[![MIT License](http://img.shields.io/badge/license-MIT-blue.svg?style=flat)](LICENSE)

[AOM(Alliance of Open Media)][aom]'s [AV1 video codec bitstream][av1-spec] parser.

The program reads AV1 bitstreams, parses header-level syntax elements, and analyzes the high-level structure of the coded video sequence.

This project is not intended to decode video frames.

[aom]: https://aomedia.org/
[av1-spec]: https://aomedia.org/av1/specification/


## Usage (Example)
Run with maximum verbose output:
```
$ cargo run stream/parkjoy.webm -vvv
...
```

(The semantics of each syntax element are defined in AV1 specification. Enjoy it! :P)


## Details
Supported file formats:
- Raw bitstream (Low overhead bitstream format)
- [IVF format][ivf]
- [WebM format][webm] ("V_AV1" codec)

[ivf]: https://wiki.multimedia.cx/index.php/IVF
[webm]: https://www.webmproject.org/

Supported OBU types:
- OBU_SEQUENCE_HEADER
- OBU_TEMPORAL_DELIMITER (no payload)
- OBU_FRAME_HEADER
- OBU_FRAME (header part only)
- OBU_TILE_LIST


## License
MIT License
