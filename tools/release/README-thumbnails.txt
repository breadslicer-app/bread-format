Thumbnails of .bread files in Windows Explorer
==============================================

Once it is registered, Explorer shows the picture a .bread file carries as its
thumbnail. Windows 10 and 11, 64-bit.

To turn it on:  run register.cmd (no administrator needed: it registers the
                handler for the current user only).
To turn it off: run unregister.cmd.

Keep this folder where it is while the thumbnails are on: Windows loads
bread_thumbnails.dll from here. To move it, run unregister.cmd, move the
folder, and run register.cmd again.

The same by hand, in a command prompt in this folder:
    regsvr32 bread_thumbnails.dll        (turn on)
    regsvr32 /u bread_thumbnails.dll     (turn off)

Explorer keeps thumbnails in a cache: a file it already showed with an icon
may keep it until the cache is cleared (Disk Cleanup, "Thumbnails").

The handler only reads the thumbnail at the start of a file: it never opens
the model itself, however large the file is.

More, and the source: https://github.com/breadslicer-app/bread-format
License: Apache-2.0 (LICENSE.txt). Notices of the libraries built in: NOTICE.txt.
