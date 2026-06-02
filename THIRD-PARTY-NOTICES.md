# Third-Party Notices

`codecpod` is distributed under the GNU Lesser General Public License v2.1 or
later (see [`LICENSE`](LICENSE)). It statically links several third-party
libraries, which are built from source at build time by `build.rs`.

The source for each linked library is **not** vendored in this repository.
Instead, `build.rs` downloads each upstream release tarball at build time,
pinned to an exact version and verified against a recorded SHA-256 checksum
(see the URL and `sha256` for each dependency in `build.rs`). These pinned
upstream tarballs constitute the complete corresponding source code for each
linked library, so that recipients can obtain the exact source, modify a
library, and relink to produce a modified binary, as contemplated by the LGPL.

The full license text of each component is preserved in this repository under
[`licenses/`](licenses/) at the path indicated below.

---

## FFmpeg

- Version: 8.1.1
- Upstream: https://ffmpeg.org/
- License: LGPL-2.1-or-later (as combined)
- License text: [`licenses/ffmpeg-8.1.1/COPYING.LGPLv2.1`](licenses/ffmpeg-8.1.1/COPYING.LGPLv2.1),
  overview in [`licenses/ffmpeg-8.1.1/LICENSE.md`](licenses/ffmpeg-8.1.1/LICENSE.md)

Most files in FFmpeg are under the LGPL v2.1+; some are under MIT/X11/BSD-style
licenses. In combination the LGPL v2.1+ applies. The optional GPL components of
FFmpeg are **not** enabled by this project — `build.rs` does not pass
`--enable-gpl` or `--enable-nonfree` — so the FFmpeg build linked here remains
LGPL-licensed.

The following FFmpeg static libraries are linked: `libavformat`, `libavcodec`,
`libswresample`, `libavutil`.

## LAME

- Version: 3.100
- Upstream: https://lame.sourceforge.io/
- License: LGPL-2.0-or-later
- License text: [`licenses/lame-3.100/COPYING`](licenses/lame-3.100/COPYING)

Linked statically as `libmp3lame` to provide the MP3 encoder.

## libogg

- Version: 1.3.5
- Upstream: https://xiph.org/ogg/
- License: BSD-3-Clause
- License text: [`licenses/libogg-1.3.5/COPYING`](licenses/libogg-1.3.5/COPYING)

```
Copyright (c) 2002, Xiph.org Foundation

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

- Redistributions of source code must retain the above copyright
  notice, this list of conditions and the following disclaimer.
- Redistributions in binary form must reproduce the above copyright
  notice, this list of conditions and the following disclaimer in the
  documentation and/or other materials provided with the distribution.
- Neither the name of the Xiph.org Foundation nor the names of its
  contributors may be used to endorse or promote products derived from
  this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. ...
```

(Full text in the file referenced above.)

## libvorbis

- Version: 1.3.7
- Upstream: https://xiph.org/vorbis/
- License: BSD-3-Clause
- License text: [`licenses/libvorbis-1.3.7/COPYING`](licenses/libvorbis-1.3.7/COPYING)

```
Copyright (c) 2002-2020 Xiph.org Foundation
```

Same BSD-3-Clause terms as libogg above; full text in the referenced file.
Linked statically as `libvorbis` / `libvorbisenc`.

## Opus

- Version: 1.5.2
- Upstream: https://opus-codec.org/
- License: BSD-3-Clause
- License text: [`licenses/opus-1.5.2/COPYING`](licenses/opus-1.5.2/COPYING)

```
Copyright 2001-2023 Xiph.Org, Skype Limited, Octasic,
                    Jean-Marc Valin, Timothy B. Terriberry,
                    CSIRO, Gregory Maxwell, Mark Borgerding,
                    Erik de Castro Lopo, Mozilla, Amazon
```

Same BSD-3-Clause terms; full text in the referenced file. Linked statically as
`libopus`.
