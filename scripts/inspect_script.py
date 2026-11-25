with open('src/workspace.html', 'r', encoding='utf-8') as f:
    curr = f.read()

s_idx = curr.find('<script>')
print('Script tag in current:', s_idx)
print('Length of current script:', len(curr) - s_idx)
