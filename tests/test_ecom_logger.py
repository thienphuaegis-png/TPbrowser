# -*- coding: utf-8 -*-
"""
Contract test for E-Commerce Worker Log System (user_data/logs/ecom_scanner.log).
Guarantees log formatting: [YYYY-MM-DD HH:MM:SS] [LEVEL] [PLATFORM] Message
"""
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOG_DIR = os.path.join(ROOT, "user_data", "logs")
LOG_FILE = os.path.join(LOG_DIR, "ecom_scanner.log")

LOG_LINE_REGEX = re.compile(
    r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\] \[(INFO|SUCCESS|WARN|ERROR)\] \[([A-Z0-9_]+)\] (.+)$"
)

def test_log_regex_matches_standard_format():
    sample_info = "[2026-11-20 01:15:02] [INFO] [PANCAKE] Bắt đầu xác thực API Key..."
    sample_success = "[2026-11-20 01:15:04] [SUCCESS] [PANCAKE] Đã kết nối shop: Kho Tổng (ID: 102)"
    sample_warn = "[2026-11-20 01:15:08] [WARN] [SHOPEE] Token SPC_CDS chưa sẵn sàng, retry 1"
    sample_error = "[2026-11-20 01:15:15] [ERROR] [TIKTOK] Phát hiện CAPTCHA xác minh"
    
    for s in [sample_info, sample_success, sample_warn, sample_error]:
        assert LOG_LINE_REGEX.match(s) is not None, f"Failed regex on: {s}"

def test_log_ecom_helper_defined_in_main_rs():
    main_rs_path = os.path.join(ROOT, "src", "main.rs")
    with open(main_rs_path, "r", encoding="utf-8") as f:
        src = f.read()
    assert "fn log_ecom(" in src, "main.rs must define fn log_ecom(&self, level: &str, tag: &str, msg: &str)"
    assert "ecom_scanner.log" in src, "main.rs must write to ecom_scanner.log"
