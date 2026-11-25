import subprocess

head = subprocess.check_output(
    ['C:/Users/thien/AppData/Local/hermes/git/cmd/git.exe', 'show', 'HEAD:src/workspace.html'],
    text=True, encoding='utf-8'
)

with open('src/workspace.html', 'w', encoding='utf-8') as f:
    f.write(head)

res = subprocess.run(
    ['C:/Users/thien/AppData/Local/Programs/Python/Python312/python.exe', '-m', 'pytest', '-q', 'tests/test_webview_ecom_scan.py'],
    capture_output=True, text=True, encoding='utf-8'
)
print('STDOUT:', res.stdout)
print('STDERR:', res.stderr)
print('Return code:', res.returncode)
