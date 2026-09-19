@echo off
if exist "%~dp0..\src-tauri\target\release\git-view.exe" (
  "%~dp0..\src-tauri\target\release\git-view.exe" %*
  exit /b %errorlevel%
)
"%~dp0..\src-tauri\target\debug\git-view.exe" %*
exit /b %errorlevel%
