use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
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
        (
            monitor.x + (self.x * monitor.width as f32).round() as i32,
            monitor.y + (self.y * monitor.height as f32).round() as i32,
            (self.width * monitor.width as f32).round().max(1.0) as u32,
            (self.height * monitor.height as f32).round().max(1.0) as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scales_with_resolution_and_negative_offsets() {
        let monitor = Monitor {
            name: "DP-1".into(),
            x: -2560,
            y: -200,
            width: 2560,
            height: 1440,
            scale: 1.5,
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
