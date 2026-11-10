@echo off
chcp 65001 >nul
echo [1/3] Dong bo CodeGraph AST...
if exist "C:\Users\thien\AppData\Local\codegraph\current\bin\codegraph.cmd" (
    call "C:\Users\thien\AppData\Local\codegraph\current\bin\codegraph.cmd" sync .
) else (
    call codegraph.cmd sync .
)

echo [2/3] Cap nhat PROJECT_STATE.json cho AI Agent...
python scripts\generate_checkpoint.py

echo [3/3] Kiem tra so do Archify...
if exist ".ai\checkpoints\diagrams\*.html" echo   [OK] So do Archify san sang.
echo [XONG] Checkpoint da san sang cho phien lam viec tiep theo!
