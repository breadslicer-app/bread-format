Bread Converter
===============

Converts 3D models between STL and .bread, one at a time or whole folders.
.bread is an open 3D file format: it keeps every vertex bit for bit and takes
much less room than STL.

How to use it
-------------
1. Open "Bread Converter.exe". There is nothing to install.
2. Drop files or folders on the window, or add them with its buttons.
3. Choose which way to convert and where to write, and convert.

With "Convert only", nothing is written: you see how much room the files
would take and how much is saved, and "Export" then writes them where you
choose.

The window is in Spanish when the system is, and in English otherwise; the
language can be changed in the window.

The program is not signed: the first time, Windows may show "Windows protected
your PC". Choose "More info", then "Run anyway".

It remembers its options in %APPDATA%\Bread Converter\settings.txt. To remove
it completely, delete that folder and this one.

More, and the source: https://github.com/breadslicer-app/bread-format
Licenses: the "licenses" folder (Apache-2.0, and the notices of the libraries
built in). The window is made with Slint (https://slint.dev).
