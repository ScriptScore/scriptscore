@echo off
rem SPDX-License-Identifier: AGPL-3.0-only
setlocal

set "SCRIPT_DIR=%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%desktop\scripts\dev-desktop.ps1" %*
exit /b %ERRORLEVEL%
