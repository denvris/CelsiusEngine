@echo off
setlocal
pushd "%~dp0" || exit /b 1

if not defined CELSIUS_OPENXR_LOADER if exist "%~dp0openxr_loader.dll" set "CELSIUS_OPENXR_LOADER=%~dp0openxr_loader.dll"
if not defined CELSIUS_OPENXR_LOADER if exist "%ProgramFiles(x86)%\Steam\steamapps\common\SteamVR\bin\win64\openxr_loader.dll" set "CELSIUS_OPENXR_LOADER=%ProgramFiles(x86)%\Steam\steamapps\common\SteamVR\bin\win64\openxr_loader.dll"

"%~dp0baseplate.exe" --lab --assets "%~dp0assets\reflection-lab" %*
set "CELSIUS_EXIT=%errorlevel%"
if not "%CELSIUS_EXIT%"=="0" (
    echo.
    echo For Quest Link and reflection lab setup, see README.md.
    pause
)
popd
exit /b %CELSIUS_EXIT%
