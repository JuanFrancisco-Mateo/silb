@echo off
setlocal
set "DB=C:\Program Files (x86)\Microsoft Visual Studio\2019\Community\VC\Tools\MSVC\14.29.30133\bin\Hostx64\x64\dumpbin.exe"
"%DB%" /nologo /HEADERS %1
endlocal