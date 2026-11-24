@echo off
rem Stops showing thumbnails of .bread files in Explorer, for the current user.
regsvr32 /u "%~dp0bread_thumbnails.dll"
