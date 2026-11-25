with open(r'C:\Users\thien\.gemini\antigravity\brain\9245228e-0d1b-425e-ad1c-e95e9fa735d7\.system_generated\steps\112\output.txt', 'r', encoding='utf-8') as f:
    diff_text = f.read()

diff_idx = diff_text.find('diff --git a/src/workspace.html')
diff_body = diff_text[diff_idx:]

lines = diff_body.splitlines()
user_lines = []
for line in lines:
    if line.startswith('+++'):
        continue
    if line.startswith('+'):
        user_lines.append(line[1:])
    elif line.startswith(' '):
        user_lines.append(line[1:])

user_content = '\n'.join(user_lines)
with open('.ai/checkpoints/user_workspace.html', 'w', encoding='utf-8') as f:
    f.write(user_content)

print('Clean user_workspace.html length:', len(user_content), 'lines:', len(user_lines))
