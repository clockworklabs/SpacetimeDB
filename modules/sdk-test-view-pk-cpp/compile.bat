@echo off
setlocal
pushd "%~dp0"
call emcmake cmake -B build .
if errorlevel 1 goto :done
call cmake --build build
:done
set ERR=%errorlevel%
popd
exit /b %ERR%
