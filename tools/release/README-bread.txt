bread
=====

A command-line program that converts STL to .bread and back, and looks inside
.bread files. .bread is an open 3D file format: it keeps every vertex bit for
bit and takes much less room than STL.

    bread model.stl                         # model.bread, next to it
    bread model.bread                       # model.stl, next to it
    bread from-stl a.stl b.stl -o kit.bread --name "A kit"
    bread to-stl kit.bread -o kit.stl
    bread info kit.bread
    bread check kit.bread other.bread

Given files alone, bread converts each by its extension and writes the result
next to it, so on Windows .stl and .bread files can be dropped on bread.exe.
It never writes over a file that is already there unless told to with --force.
`bread --help` lists everything it does.

The program is not signed:
- On Windows, the first run may show "Windows protected your PC": choose
  "More info", then "Run anyway".
- On macOS, remove the download's quarantine once, in the folder you unpacked:
      xattr -d com.apple.quarantine bread

More, and the source: https://github.com/breadslicer-app/bread-format
License: Apache-2.0 (LICENSE.txt). Notices of the libraries built in: NOTICE.txt.
