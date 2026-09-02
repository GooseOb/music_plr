@echo off
:wait
tasklist /FI "PID eq {PID}" 2>NUL | findstr {PID} >NUL
if %errorlevel% == 0 ( timeout /t 1 /nobreak >NUL & goto wait )
move /Y "{NEW}" "{OLD}"
start "" "{OLD}"
