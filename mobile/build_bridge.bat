@echo off
setlocal enabledelayedexpansion

echo ===================================================
echo Building AdeshLang Mobile Bridge for Android (NDK)
echo ===================================================

if "%ANDROID_NDK_HOME%"=="" (
    if exist "C:\Android\Sdk\ndk\27.0.11902837" (
        set "ANDROID_NDK_HOME=C:\Android\Sdk\ndk\27.0.11902837"
    ) else if exist "%LOCALAPPDATA%\Android\Sdk\ndk" (
        for /d %%D in ("%LOCALAPPDATA%\Android\Sdk\ndk\*") do set "ANDROID_NDK_HOME=%%D"
    )
)

if "%ANDROID_NDK_HOME%"=="" (
    echo [ERROR] ANDROID_NDK_HOME is not set and could not be detected.
    echo Please set ANDROID_NDK_HOME to your NDK installation directory.
    exit /b 1
)

echo Using NDK at: %ANDROID_NDK_HOME%

set SCRIPT_DIR=%~dp0
set OUTPUT_DIR=%SCRIPT_DIR%flutter\android\app\src\main\jniLibs

echo Target output directory: %OUTPUT_DIR%

cargo ndk -t arm64-v8a -t x86_64 -o "%OUTPUT_DIR%" --manifest-path "%SCRIPT_DIR%bridge\Cargo.toml" build --release
if errorlevel 1 (
    echo [ERROR] cargo ndk build failed.
    exit /b %errorlevel%
)

echo [SUCCESS] AdeshLang Mobile Bridge libraries built and copied successfully!
