use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    /// Area left for windows by panels and docks; absent from X11 and older GNOME extensions.
    #[serde(default)]
    pub work_area: Option<Area>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Monitor {
    /// The rectangle zones are laid out in: the work area if it lies within the monitor, else the whole monitor.
    pub fn usable(&self) -> Area {
        let full = Area {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        };
        self.work_area
            .filter(|area| {
                area.width > 0
                    && area.height > 0
                    && area.x >= full.x
                    && area.y >= full.y
                    && i64::from(area.x) + i64::from(area.width)
                        <= i64::from(full.x) + i64::from(full.width)
                    && i64::from(area.y) + i64::from(area.height)
                        <= i64::from(full.y) + i64::from(full.height)
            })
            .unwrap_or(full)
    }

    pub fn display_name(&self, index: usize) -> String {
        format!("Monitor {} · {}", index + 1, self.name)
    }
}

pub fn sort_for_display(monitors: &mut [Monitor]) {
    monitors.sort_by(|a, b| (a.y, a.x, &a.name).cmp(&(b.y, b.x, &b.name)));
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        }
    }
}

impl Bounds {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= 1.00001
            && self.y + self.height <= 1.00001
    }

    pub fn pixels(self, monitor: &Monitor) -> (i32, i32, u32, u32) {
        // Round shared edges, so adjacent percentage zones stay adjacent at
        // resolutions that are not evenly divisible by their proportions.
        let edges = |start: f32, size: f32, extent: u32| {
            let left = ((f64::from(start) * f64::from(extent)).round() as u32)
                .min(extent.saturating_sub(1));
            let right = (f64::from((start + size).min(1.0)) * f64::from(extent)).round() as u32;
            (left, right.saturating_sub(left).max(1))
        };
        let area = monitor.usable();
        let (x, width) = edges(self.x, self.width, area.width);
        let (y, height) = edges(self.y, self.height, area.height);
        (area.x + x as i32, area.y + y as i32, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_fill_the_work_area_and_ignore_an_invalid_one() {
        let mut monitor = Monitor {
            name: "eDP-1".into(),
            x: 1920,
            y: 0,
            width: 2560,
            height: 1600,
            scale: 1.0,
            work_area: Some(Area {
                x: 1920,
                y: 32,
                width: 2560,
                height: 1488,
            }),
        };
        let full = Bounds {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        assert_eq!(full.pixels(&monitor), (1920, 32, 2560, 1488));
        let right = Bounds {
            x: 0.5,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        };
        assert_eq!(right.pixels(&monitor), (3200, 32, 1280, 1488));
        // A work area reaching outside its monitor is not trusted.
        monitor.work_area = Some(Area {
            x: 0,
            y: 0,
            width: 4480,
            height: 1600,
        });
        assert_eq!(full.pixels(&monitor), (1920, 0, 2560, 1600));
        monitor.work_area = None;
        assert_eq!(full.pixels(&monitor), (1920, 0, 2560, 1600));
    }
    #[test]
    fn adjacent_zones_share_pixel_edges_after_resolution_changes() {
        for (width, height, scale) in [(1366, 768, 1.0), (2560, 1440, 1.5), (3840, 2160, 2.0)] {
            let monitor = Monitor {
                name: "DP-1".into(),
                x: -width,
                y: -height,
                width: width as u32,
                height: height as u32,
                scale,
                work_area: None,
            };
            let zones: Vec<_> = (0..3)
                .map(|index| {
                    Bounds {
                        x: index as f32 / 3.0,
                        y: 0.0,
                        width: 1.0 / 3.0,
                        height: 1.0,
                    }
                    .pixels(&monitor)
                })
                .collect();
            assert_eq!(zones[0].0, monitor.x);
            for pair in zones.windows(2) {
                assert_eq!(
                    pair[0].0 + pair[0].2 as i32,
                    pair[1].0,
                    "gap at {width} pixels"
                );
            }
            assert_eq!(zones[2].0 + zones[2].2 as i32, monitor.x + width);
            assert!(zones.iter().all(|zone| zone.3 == monitor.height));
            let rows: Vec<_> = (0..3)
                .map(|index| {
                    Bounds {
                        x: 0.0,
                        y: index as f32 / 3.0,
                        width: 1.0,
                        height: 1.0 / 3.0,
                    }
                    .pixels(&monitor)
                })
                .collect();
            assert_eq!(rows[0].1, monitor.y);
            for pair in rows.windows(2) {
                assert_eq!(pair[0].1 + pair[0].3 as i32, pair[1].1);
            }
            assert_eq!(rows[2].1 + rows[2].3 as i32, monitor.y + height);
            assert!(rows.iter().all(|row| row.2 == monitor.width));
        }
    }

    #[test]
    fn subpixel_zone_at_far_edge_remains_inside_monitor() {
        let monitor = Monitor {
            name: "DP-1".into(),
            x: -1366,
            y: -768,
            width: 1366,
            height: 768,
            scale: 1.0,
            work_area: None,
        };
        let bounds = Bounds {
            x: 0.999999,
            y: 0.999999,
            width: 0.000001,
            height: 0.000001,
        };
        assert!(bounds.valid());
        assert_eq!(bounds.pixels(&monitor), (-1, -1, 1, 1));
    }

    #[test]
    fn scales_with_resolution_and_negative_offsets() {
        let monitor = Monitor {
            name: "DP-1".into(),
            x: -2560,
            y: -200,
            width: 2560,
            height: 1440,
            scale: 1.5,
            work_area: None,
        };
        let bounds = Bounds {
            x: 0.5,
            y: 0.25,
            width: 0.5,
            height: 0.75,
        };
        assert_eq!(bounds.pixels(&monitor), (-1280, 160, 1280, 1080));
        assert!(!Bounds {
            width: f32::NAN,
            ..bounds
        }
        .valid());
        assert!(!Bounds { x: 0.9, ..bounds }.valid());
    }
}
