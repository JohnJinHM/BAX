//! 悬浮窗贴边的几何计算，全部是物理像素。
//!
//! 做法参考原型 ba-elo-overlay 的 `_snap` 与 `_apply_dock`：拖动松手后离屏幕边缘不远就贴过去；
//! 展开、收起或缩放让窗口变大小时，重新贴到原来那一侧。这里按边各自判断，不像原型那样总是贴左右两侧之一，
//! 也不额外记住贴的是哪一侧：窗口贴着哪条边，直接从位置看出来。

/// 屏幕上的矩形，左上角加宽高。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    fn right(&self) -> i32 {
        self.x + self.width
    }

    fn bottom(&self) -> i32 {
        self.y + self.height
    }
}

/// 换算误差内算作贴着。
const FLUSH: i32 = 1;

/// 拖动松手后的位置。窗口的某条边离工作区同侧的边不超过 `distance` 时贴过去，左右、上下分别判断；
/// 都不近时位置不变。
pub fn snap(window: Rect, area: Rect, distance: i32) -> (i32, i32) {
    let x = if (window.x - area.x).abs() <= distance {
        area.x
    } else if (area.right() - window.right()).abs() <= distance {
        area.right() - window.width
    } else {
        window.x
    };
    let y = if (window.y - area.y).abs() <= distance {
        area.y
    } else if (area.bottom() - window.bottom()).abs() <= distance {
        area.bottom() - window.height
    } else {
        window.y
    };
    (x, y)
}

/// 窗口改成新的大小以后放在哪里：原来贴着右边或下边的继续贴着，再挪回工作区里，
/// 免得贴着下边展开时窗口长到任务栏下面。贴着左边、上边的本来就不动。
pub fn resize(window: Rect, width: i32, height: i32, area: Rect) -> (i32, i32) {
    let x = if (window.right() - area.right()).abs() <= FLUSH {
        area.right() - width
    } else {
        window.x
    };
    let y = if (window.bottom() - area.bottom()).abs() <= FLUSH {
        area.bottom() - height
    } else {
        window.y
    };
    (
        inside(x, width, area.x, area.right()),
        inside(y, height, area.y, area.bottom()),
    )
}

/// 一个方向上挪进 `[start, end)`；比工作区还长时对齐起点。
fn inside(position: i32, length: i32, start: i32, end: i32) -> i32 {
    position.min(end - length).max(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1920×1080 的屏幕，下面 48 像素是任务栏。
    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1032,
    };

    fn window(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 340,
            height: 400,
        }
    }

    #[test]
    fn snaps_each_axis_to_the_nearest_edge_within_distance() {
        assert_eq!(snap(window(10, 300), AREA, 18), (0, 300));
        assert_eq!(snap(window(1570, 12), AREA, 18), (1580, 0));
        assert_eq!(snap(window(800, 620), AREA, 18), (800, 632));
        // 稍微拖出屏幕外也算靠近
        assert_eq!(snap(window(-15, -5), AREA, 18), (0, 0));
    }

    #[test]
    fn leaves_the_window_alone_away_from_edges() {
        assert_eq!(snap(window(19, 19), AREA, 18), (19, 19));
        assert_eq!(snap(window(-40, 300), AREA, 18), (-40, 300));
    }

    #[test]
    fn works_on_a_secondary_monitor_with_an_offset_origin() {
        let area = Rect {
            x: -1280,
            y: 200,
            width: 1280,
            height: 984,
        };
        assert_eq!(snap(window(-1270, 210), area, 18), (-1280, 200));
        assert_eq!(snap(window(-350, 790), area, 18), (-340, 784));
    }

    #[test]
    fn resizing_keeps_right_and_bottom_edges() {
        // 贴着右下角收起
        let snapped = window(1580, 632);
        assert_eq!(resize(snapped, 340, 120, AREA), (1580, 912));
        // 贴着右上角展开，只保持右边
        let top_right = window(1580, 0);
        assert_eq!(resize(top_right, 408, 600, AREA), (1512, 0));
    }

    #[test]
    fn resizing_pulls_a_growing_window_back_into_the_work_area() {
        let low = window(600, 700);
        assert_eq!(resize(low, 340, 500, AREA), (600, 532));
        let taller_than_screen = window(600, 100);
        assert_eq!(resize(taller_than_screen, 340, 1200, AREA), (600, 0));
    }

    #[test]
    fn resizing_a_free_window_inside_the_area_keeps_its_corner() {
        assert_eq!(resize(window(600, 100), 340, 500, AREA), (600, 100));
    }
}
