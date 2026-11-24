@echo off
setlocal
pushd "%~dp0" || exit /b 1

if not defined CELSIUS_OPENXR_LOADER if exist "%~dp0openxr_loader.dll" set "CELSIUS_OPENXR_LOADER=%~dp0openxr_loader.dll"
if not defined CELSIUS_OPENXR_LOADER if exist "%ProgramFiles(x86)%\Steam\steamapps\common\SteamVR\bin\win64\openxr_loader.dll" set "CELSIUS_OPENXR_LOADER=%ProgramFiles(x86)%\Steam\steamapps\common\SteamVR\bin\win64\openxr_loader.dll"

set "CELSIUS_LOG=%TEMP%\CelsiusEngine-ReflectionLab.log"
echo Starting Celsius Engine Reflection Lab. Log: "%CELSIUS_LOG%"
> "%CELSIUS_LOG%" echo Executable: "%~dp0baseplate.exe"
if defined CELSIUS_OPENXR_LOADER >> "%CELSIUS_LOG%" echo Loader: "%CELSIUS_OPENXR_LOADER%"
if not exist "%~dp0baseplate.exe" (
    >> "%CELSIUS_LOG%" echo The executable is missing. Extract the full Reflection Lab ZIP into one folder.
    set "CELSIUS_EXIT=1"
    goto report
)
"%~dp0baseplate.exe" --help 2>nul | findstr /L /C:"--lab" >nul
if errorlevel 1 (
    >> "%CELSIUS_LOG%" echo This executable does not support Reflection Lab. Extract the full Reflection Lab ZIP into one folder.
    set "CELSIUS_EXIT=1"
    goto report
)
"%~dp0baseplate.exe" --lab --assets "%~dp0assets\reflection-lab" %* >> "%CELSIUS_LOG%" 2>&1
set "CELSIUS_EXIT=%errorlevel%"

:report
type "%CELSIUS_LOG%"
if not "%CELSIUS_EXIT%"=="0" (
    echo.
    echo This launch failed. Log saved to "%CELSIUS_LOG%".
    pause
)
popd
exit /b %CELSIUS_EXIT%
