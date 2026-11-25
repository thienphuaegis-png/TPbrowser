import re

path = "src/workspace.html"
with open(path, "r", encoding="utf-8") as f:
    content = f.read()

# 1. Locate broken button attribute fragment
start_marker = '<button id="tp_skip_intro" type="button" style="position:absolute;top:18px;right:22px;background:rgba(2'
idx_button_frag = content.find(start_marker)
if idx_button_frag == -1:
    raise SystemExit("start_marker not found")

# position right after 'rgba(2' (end of the broken fragment)
idx_js_block_start = idx_button_frag + len(start_marker)

# 2. Locate where style attribute resumes: '55,255,255,0.08);color:#f1f5f9;'
end_marker = "55,255,255,0.08);color:#f1f5f9;"
idx_resume = content.find(end_marker, idx_js_block_start)
if idx_resume == -1:
    raise SystemExit("end_marker not found")

# JS block is everything between idx_js_block_start and idx_resume, trimmed of leading/trailing blank lines
js_block_raw = content[idx_js_block_start:idx_resume]
js_block = js_block_raw.strip("\n")

# 3. Build fixed content: remove JS block, glue 'rgba(2' + '55,255,255,0.08)...' = 'rgba(255,255,255,0.08)...'
fixed_tail = content[idx_resume:]
before_button = content[:idx_button_frag]
new_button_full = start_marker + end_marker[0:2]  # 'rgba(2' + '55' -> not needed since end_marker already starts with '55'
# simplest: just concatenate start_marker (ends at 'rgba(2') directly with content from idx_resume (starts with '55,255,...')
glued = before_button + start_marker + content[idx_resume:]

# 4. Insert JS block (wrapped in <script>) right before the Cinematic Intro comment
intro_comment = "<!-- Cinematic Workspace Reveal Intro -->"
idx_intro = glued.find(intro_comment)
if idx_intro == -1:
    raise SystemExit("intro comment not found")

script_insert = "<script>\n" + js_block + "\n  </script>\n  "
final_content = glued[:idx_intro] + script_insert + glued[idx_intro:]

# 5. Fix typo stat_rev -> stat-rev inside clearPlatformOnlyShopeeValues array
final_content = final_content.replace(
    '["sp_pas_sales", "sp_overview_pas", "sp_overview_health_penalty", "stat_rev"]',
    '["sp_pas_sales", "sp_overview_pas", "sp_overview_health_penalty", "stat-rev"]',
)

with open(path, "w", encoding="utf-8") as f:
    f.write(final_content)

print("OK. Original length:", len(content), "New length:", len(final_content))
print("JS block length:", len(js_block))
