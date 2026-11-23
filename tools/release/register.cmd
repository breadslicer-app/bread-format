@echo off
rem Shows thumbnails of .bread files in Explorer, for the current user.
rem No administrator needed. Run unregister.cmd to undo it.
regsvr32 "%~dp0bread_thumbnails.dll"
