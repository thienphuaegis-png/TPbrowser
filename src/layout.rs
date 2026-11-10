/// Bộ tính toán bố cục khung hình cho TPbrowser:
/// - Thanh Quản lý Tab: Chiều dọc bên tay trái (`VerticalLeft`) hoặc Chiều ngang phía trên (`HorizontalTop`).
/// - Ngăn AI Cowork: Mở rộng bên tay phải (`RightDock`) hoặc Thu gọn thành Bong bóng chat nổi (`BubbleMinimized`).
/// - Vùng trang web: Hiển thị 1 Tab toàn vùng (`SingleTab`) hoặc Chia khung song song (`SplitBsp` - Spiral / Columns).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabOrientation {
    VerticalLeft,
    HorizontalTop,
}

impl TabOrientation {
    pub fn as_str(self) -> &'static str {
        match self {
            TabOrientation::VerticalLeft => "vertical",
            TabOrientation::HorizontalTop => "horizontal",
        }
    }

    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("horizontal") {
            TabOrientation::HorizontalTop
        } else {
            TabOrientation::VerticalLeft
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            TabOrientation::VerticalLeft => TabOrientation::HorizontalTop,
            TabOrientation::HorizontalTop => TabOrientation::VerticalLeft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDisplayMode {
    RightDock,
    BubbleMinimized,
}

impl AiDisplayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AiDisplayMode::RightDock => "dock",
            AiDisplayMode::BubbleMinimized => "bubble",
        }
    }

    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("bubble") {
            AiDisplayMode::BubbleMinimized
        } else {
            AiDisplayMode::RightDock
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentViewMode {
    SingleTab,
    SpiralBsp,
    ColumnsBsp,
}

impl ContentViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ContentViewMode::SingleTab => "single",
            ContentViewMode::SpiralBsp => "spiral",
            ContentViewMode::ColumnsBsp => "columns",
        }
    }

    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("spiral") {
            ContentViewMode::SpiralBsp
        } else if s.eq_ignore_ascii_case("columns") {
            ContentViewMode::ColumnsBsp
        } else {
            ContentViewMode::SingleTab
        }
    }

    pub fn next(self) -> Self {
        match self {
            ContentViewMode::SingleTab => ContentViewMode::SpiralBsp,
            ContentViewMode::SpiralBsp => ContentViewMode::ColumnsBsp,
            ContentViewMode::ColumnsBsp => ContentViewMode::SingleTab,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellLayout {
    pub tab_bar: PaneRect,
    pub content_area: PaneRect,
    pub ai_dock: Option<PaneRect>,
}

pub const LEFT_SIDEBAR_WIDTH: u32 = 228;
pub const LEFT_SIDEBAR_COLLAPSED_WIDTH: u32 = 54;
pub const TOP_TABBAR_HEIGHT: u32 = 82;
pub const RIGHT_AI_DOCK_WIDTH: u32 = 360;

pub fn compute_shell_layout(
    window_w: u32,
    window_h: u32,
    tab_orientation: TabOrientation,
    sidebar_collapsed: bool,
    ai_mode: AiDisplayMode,
    ai_dock_width: u32,
) -> ShellLayout {
    let safe_w = window_w.max(320);
    let safe_h = window_h.max(240);

    let (tab_bar, rem_x, rem_y, rem_w, rem_h) = match tab_orientation {
        TabOrientation::VerticalLeft => {
            let side_w = if sidebar_collapsed {
                LEFT_SIDEBAR_COLLAPSED_WIDTH
            } else {
                LEFT_SIDEBAR_WIDTH
            };
            let side_w = side_w.min(safe_w / 2);
            (
                PaneRect {
                    x: 0,
                    y: 0,
                    width: side_w,
                    height: safe_h,
                },
                side_w as i32,
                0,
                safe_w.saturating_sub(side_w),
                safe_h,
            )
        }
        TabOrientation::HorizontalTop => {
            let top_h = TOP_TABBAR_HEIGHT.min(safe_h / 2);
            (
                PaneRect {
                    x: 0,
                    y: 0,
                    width: safe_w,
                    height: top_h,
                },
                0,
                top_h as i32,
                safe_w,
                safe_h.saturating_sub(top_h),
            )
        }
    };

    match ai_mode {
        AiDisplayMode::RightDock => {
            let dock_w = ai_dock_width.clamp(260, 520).min(rem_w / 2);
            let content_w = rem_w.saturating_sub(dock_w).max(100);
            let content_area = PaneRect {
                x: rem_x,
                y: rem_y,
                width: content_w,
                height: rem_h,
            };
            let ai_dock = Some(PaneRect {
                x: rem_x + content_w as i32,
                y: rem_y,
                width: dock_w,
                height: rem_h,
            });
            ShellLayout {
                tab_bar,
                content_area,
                ai_dock,
            }
        }
        AiDisplayMode::BubbleMinimized => ShellLayout {
            tab_bar,
            content_area: PaneRect {
                x: rem_x,
                y: rem_y,
                width: rem_w.max(100),
                height: rem_h,
            },
            ai_dock: None,
        },
    }
}

/// Tính tọa độ cho các Tab bên trong `content_area` khi bật chế độ Chia khung BSP (Spiral hoặc Columns)
pub fn compute_content_panes(
    area: PaneRect,
    view_mode: ContentViewMode,
    count: usize,
    master_ratio: f32,
    gap: u32,
) -> Vec<PaneRect> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 || view_mode == ContentViewMode::SingleTab {
        return vec![area];
    }

    match view_mode {
        ContentViewMode::SingleTab => vec![area],
        ContentViewMode::ColumnsBsp => compute_columns_in_area(area, count, gap),
        ContentViewMode::SpiralBsp => compute_spiral_in_area(area, count, master_ratio, gap),
    }
}

pub fn compute_columns_in_area(area: PaneRect, count: usize, gap: u32) -> Vec<PaneRect> {
    let total_gap = gap.saturating_mul((count.saturating_sub(1)) as u32);
    let avail_w = area.width.saturating_sub(total_gap).max(count as u32 * 40);
    let col_w = avail_w / (count as u32);

    let mut out = Vec::with_capacity(count);
    let mut cur_x = area.x;
    for i in 0..count {
        let w = if i == count - 1 {
            ((area.x + area.width as i32) - cur_x).max(40) as u32
        } else {
            col_w
        };
        out.push(PaneRect {
            x: cur_x,
            y: area.y,
            width: w,
            height: area.height,
        });
        cur_x += (w + gap) as i32;
    }
    out
}

pub fn compute_spiral_in_area(
    area: PaneRect,
    count: usize,
    master_ratio: f32,
    gap: u32,
) -> Vec<PaneRect> {
    let clamped_ratio = master_ratio.clamp(0.25, 0.75);
    let mut rects = Vec::with_capacity(count);
    let mut rem = area;

    for i in 0..count {
        if i == count - 1 {
            rects.push(rem);
            break;
        }
        let ratio = if i == 0 { clamped_ratio } else { 0.5 };
        match i % 4 {
            0 => {
                let avail_w = rem.width.saturating_sub(gap);
                let left_w = ((avail_w as f32) * ratio).round() as u32;
                let left_w = left_w.max(40).min(avail_w.saturating_sub(40));
                let right_w = avail_w.saturating_sub(left_w);
                rects.push(PaneRect {
                    x: rem.x,
                    y: rem.y,
                    width: left_w,
                    height: rem.height,
                });
                rem.x += (left_w + gap) as i32;
                rem.width = right_w;
            }
            1 => {
                let avail_h = rem.height.saturating_sub(gap);
                let top_h = ((avail_h as f32) * ratio).round() as u32;
                let top_h = top_h.max(40).min(avail_h.saturating_sub(40));
                let bottom_h = avail_h.saturating_sub(top_h);
                rects.push(PaneRect {
                    x: rem.x,
                    y: rem.y,
                    width: rem.width,
                    height: top_h,
                });
                rem.y += (top_h + gap) as i32;
                rem.height = bottom_h;
            }
            2 => {
                let avail_w = rem.width.saturating_sub(gap);
                let right_w = ((avail_w as f32) * ratio).round() as u32;
                let right_w = right_w.max(40).min(avail_w.saturating_sub(40));
                let left_w = avail_w.saturating_sub(right_w);
                rects.push(PaneRect {
                    x: rem.x + (left_w + gap) as i32,
                    y: rem.y,
                    width: right_w,
                    height: rem.height,
                });
                rem.width = left_w;
            }
            _ => {
                let avail_h = rem.height.saturating_sub(gap);
                let bottom_h = ((avail_h as f32) * ratio).round() as u32;
                let bottom_h = bottom_h.max(40).min(avail_h.saturating_sub(40));
                let top_h = avail_h.saturating_sub(bottom_h);
                rects.push(PaneRect {
                    x: rem.x,
                    y: rem.y + (top_h + gap) as i32,
                    width: rem.width,
                    height: bottom_h,
                });
                rem.height = top_h;
            }
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertical_left_with_right_ai_dock() {
        let layout = compute_shell_layout(
            1440,
            900,
            TabOrientation::VerticalLeft,
            false,
            AiDisplayMode::RightDock,
            RIGHT_AI_DOCK_WIDTH,
        );
        assert_eq!(layout.tab_bar.x, 0);
        assert_eq!(layout.tab_bar.width, LEFT_SIDEBAR_WIDTH);
        assert_eq!(layout.tab_bar.height, 900);

        let ai = layout.ai_dock.expect("AI dock should be present");
        assert_eq!(ai.width, RIGHT_AI_DOCK_WIDTH);
        assert_eq!(layout.content_area.x, LEFT_SIDEBAR_WIDTH as i32);
        assert_eq!(
            layout.content_area.width + LEFT_SIDEBAR_WIDTH + RIGHT_AI_DOCK_WIDTH,
            1440
        );
    }

    #[test]
    fn test_horizontal_top_with_minimized_bubble() {
        let layout = compute_shell_layout(
            1440,
            900,
            TabOrientation::HorizontalTop,
            false,
            AiDisplayMode::BubbleMinimized,
            RIGHT_AI_DOCK_WIDTH,
        );
        assert_eq!(layout.tab_bar.x, 0);
        assert_eq!(layout.tab_bar.y, 0);
        assert_eq!(layout.tab_bar.width, 1440);
        assert_eq!(layout.tab_bar.height, TOP_TABBAR_HEIGHT);
        assert!(layout.ai_dock.is_none());
        assert_eq!(layout.content_area.x, 0);
        assert_eq!(layout.content_area.y, TOP_TABBAR_HEIGHT as i32);
        assert_eq!(layout.content_area.width, 1440);
        assert_eq!(layout.content_area.height, 900 - TOP_TABBAR_HEIGHT);
    }

    #[test]
    fn test_spiral_split_inside_content_area() {
        let area = PaneRect {
            x: 228,
            y: 0,
            width: 1212,
            height: 900,
        };
        let rects = compute_content_panes(area, ContentViewMode::SpiralBsp, 3, 0.55, 4);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].x, 228);
        assert!(rects[1].x > rects[0].x);
        assert!(rects[2].y > rects[1].y);
    }
}
