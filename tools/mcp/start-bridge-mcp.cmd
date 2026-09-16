@echo off
rem Launcher for the Codex Status MCP server (stdio). Works from any clone.
setlocal
set "ROOT=%~dp0..\.."
if not defined CODEX_STATUS_ROOT set "CODEX_STATUS_ROOT=%ROOT%"
set "EXE=%ROOT%\bridge\target\release\bridge-mcp.exe"
if not exist "%EXE%" set "EXE=%ROOT%\bridge\target\debug\bridge-mcp.exe"
"%EXE%" %*
