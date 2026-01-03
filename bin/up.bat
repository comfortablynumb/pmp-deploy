@echo off
setlocal

set SCRIPT_DIR=%~dp0
set PROJECT_DIR=%SCRIPT_DIR%..

cd /d "%PROJECT_DIR%"

set PROFILE=%1

rem Run down first
call "%SCRIPT_DIR%down.bat" %PROFILE%

if "%PROFILE%"=="" (
    echo Starting services...
    docker compose up -d
) else (
    echo Starting services with profile: %PROFILE%
    docker compose --profile %PROFILE% up -d
)

echo Services started successfully
endlocal
