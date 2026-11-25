import subprocess

head = subprocess.check_output(
    ['C:/Users/thien/AppData/Local/hermes/git/cmd/git.exe', 'show', 'HEAD:src/workspace.html'],
    text=True, encoding='utf-8'
)

# 1. Check Platform dashboard block in HEAD CSS
idx1 = head.find('/* Platform dashboard: theme-first, no dark island or fabricated KPIs. */')
print('Platform dashboard CSS in HEAD:')
print(head[idx1:idx1+400])

# 2. Check tiktok tokens block in HEAD CSS
idx2 = head.find('html[data-theme="obsidian-gold"] body[data-platform="tiktok"]')
print('TikTok tokens CSS in HEAD:')
print(head[idx2:idx2+600])
