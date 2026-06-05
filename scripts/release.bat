@echo off
setlocal EnableExtensions

rem Windows release entrypoint.
rem Usage:
rem   scripts\release.bat 0.1.1
rem   scripts\release.bat
rem   scripts\release.bat 0.1.1 -Message "note"

set "SCRIPT_DIR=%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%release.ps1" %*
exit /b %ERRORLEVEL%
