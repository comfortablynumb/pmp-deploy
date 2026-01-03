@echo off
setlocal

set SCRIPT_DIR=%~dp0
set PROJECT_DIR=%SCRIPT_DIR%..

cd /d "%PROJECT_DIR%"

set PROFILE=%1

if "%PROFILE%"=="" (
    echo Stopping services...
    docker compose rm -f --stop
) else (
    echo Stopping services with profile: %PROFILE%
    docker compose --profile %PROFILE% rm -f --stop
)

echo Services stopped successfully
endlocal
