# av1parser
[![MIT License](http://img.shields.io/badge/license-MIT-blue.svg?style=flat)](LICENSE)

[AOM(Allianc of Open Media)][aom]'s [AV1 video codec bitstream][av1-spec] parser.

The program reads AV1 bistreams, parses header-level syntax elements and analyze high-level structure of coded sequence.

This project is not intended to decode video frames.

[aom]: https://aomedia.org/
[av1-spec]: https://aomedia.org/av1-bitstream-and-decoding-process-specification/


## Usage (Example)
Run with maxmum verbose output:
```
$ cargo run stream/parkjoy.webm -vvv
...
```

(The semantics of each syntax elements are defined in AV1 specficiation. Enjoy it! :P)


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


## License
MIT License
