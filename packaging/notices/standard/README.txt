Standard licence texts, used by packaging/mac/collect_notices.py for a bundled package that ships no licence file of its
own (its licence is named in Homebrew's formula, and the text is the same wherever the licence is used).

  LGPL-2.1.txt  GNU Lesser General Public License, version 2.1. From ffmpeg 9.0.2 (COPYING.LGPLv2.1), the ffmpeg package.
  LGPL-3.0.txt  GNU Lesser General Public License, version 3. From ffmpeg 9.0.2 (COPYING.LGPLv3), the ffmpeg package.
  GPL-2.0.txt   GNU General Public License, version 2. From ffmpeg 9.0.2 (COPYING.GPLv2), the ffmpeg package.
  GPL-3.0.txt   GNU General Public License, version 3. From ffmpeg 9.0.2 (COPYING.GPLv3), the ffmpeg package.
  BSL-1.0.txt   Boost Software License, version 1.0. From LICENSE_1_0.txt in boost-1.92.0-b2-nodocs.tar.xz, whose SHA-256
                matches the one Homebrew records (ea7b9820...d82411).
  Qt-GPL-exception-1.0.txt
                The Qt Company GPL Exception 1.0. From LICENSES/Qt-GPL-exception-1.0.txt in qtbase-everywhere-src-6.12.0.tar.xz,
                whose SHA-256 matches the one Homebrew records for qtbase (a951bd16...131307).
  GFDL-1.3-no-invariants-only.txt
                GNU Free Documentation License, version 1.3, no invariant sections. From the same Qt source, LICENSES/.
  BSD-3-Clause.txt
                BSD 3-clause licence, from the same Qt source, LICENSES/. This is Qt's SPDX template: its copyright line has
                placeholders (<year> <owner>). The copyright holders of the Qt parts are in the source files' headers.

The collector finds these by the licence name Homebrew gives a package, for example Qt's
"(GPL-3.0-only WITH Qt-GPL-exception-1.0) AND BSD-3-Clause AND GFDL-1.3-no-invariants-only".
