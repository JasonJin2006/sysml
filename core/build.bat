@echo off
rem Runs cargo with the MSVC environment loaded, from any working directory.
rem
rem cargo needs vcvars for link.exe: the Git Bash link.exe shadows the MSVC one, so a bare
rem "cargo build" fails at link time. The vcvars location differs per machine. Naming one that is
rem absent makes "call" print a path-not-found message on stderr while cargo still runs, which
rem reads as a build failure to anything that judges the build on stderr. So every candidate is
rem tested with "if exist" before it is called, and the call itself writes nothing.
setlocal
cd /d "%~dp0"
set "VCVARS="
if defined VSINSTALLDIR if exist "%VSINSTALLDIR%VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%VSINSTALLDIR%VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles%\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles%\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2019\Enterprise\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles(x86)%\Microsoft Visual Studio\2019\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2019\Professional\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles(x86)%\Microsoft Visual Studio\2019\Professional\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles(x86)%\Microsoft Visual Studio\2019\Community\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%ProgramFiles(x86)%\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "C:\BuildTools\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=C:\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "D:\Softwares\BuildTools\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=D:\Softwares\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if defined VCVARS call "%VCVARS%" >nul 2>nul
cargo %*
endlocal & exit /b %errorlevel%
