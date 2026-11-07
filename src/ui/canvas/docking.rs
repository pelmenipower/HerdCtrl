//! Percentage geometry for snapping and collision-free canvas gestures.
use crate::core::monitor::Bounds;
use eframe::egui::{pos2, Pos2, Vec2};

const EPS: f32 = 0.00001;
const GRID: [f32; 7] = [0.0, 0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 0.75, 1.0];

pub fn overlaps(a: Bounds, b: Bounds) -> bool {
    a.x < b.x + b.width - EPS
        && a.x + a.width > b.x + EPS
        && a.y < b.y + b.height - EPS
        && a.y + a.height > b.y + EPS
}

pub fn lines(others: &[Bounds]) -> (Vec<f32>, Vec<f32>) {
    let mut x = GRID.to_vec();
    let mut y = GRID.to_vec();
    for other in others {
        x.extend([other.x, other.x + other.width]);
        y.extend([other.y, other.y + other.height]);
    }
    (x, y)
}

fn snap(value: f32, lines: &[f32], extent: f32) -> f32 {
    let tolerance = (6.0 / extent.max(1.0)).min(0.04);
    lines
        .iter()
        .copied()
        .filter(|line| (value - line).abs() <= tolerance)
        .min_by(|a, b| (value - a).abs().total_cmp(&(value - b).abs()))
        .unwrap_or(value)
}

pub fn point(point: Pos2, others: &[Bounds], size: Vec2) -> Pos2 {
    let (x, y) = lines(others);
    pos2(
        snap(point.x.clamp(0.0, 1.0), &x, size.x),
        snap(point.y.clamp(0.0, 1.0), &y, size.y),
    )
}

fn bounds(start: Pos2, end: Pos2) -> Bounds {
    Bounds {
        x: start.x.min(end.x),
        y: start.y.min(end.y),
        width: (end.x - start.x).abs(),
        height: (end.y - start.y).abs(),
    }
}

pub fn draw(start: Pos2, end: Pos2, others: &[Bounds], size: Vec2) -> Option<Bounds> {
    fit(point(start, others, size), point(end, others, size), others)
}

pub fn resize(original: Bounds, end: Pos2, others: &[Bounds], size: Vec2) -> Option<Bounds> {
    let end = point(end, others, size);
    fit(
        pos2(original.x, original.y),
        pos2(end.x.max(original.x + EPS), end.y.max(original.y + EPS)),
        others,
    )
}

// Largest empty rectangle anchored at the press origin and bounded by the cursor.
// Each candidate x edge only needs one pass over the y obstacles (O(n²)).
fn fit(start: Pos2, end: Pos2, others: &[Bounds]) -> Option<Bounds> {
    let mut candidates = vec![end.x];
    for other in others {
        let edge = if end.x >= start.x {
            other.x
        } else {
            other.x + other.width
        };
        if edge >= start.x.min(end.x) && edge <= start.x.max(end.x) {
            candidates.push(edge);
        }
    }
    let mut best: Option<Bounds> = None;
    for x in candidates {
        let mut y = end.y;
        let mut valid = true;
        for other in others {
            if !overlaps(bounds(start, pos2(x, y)), *other) {
                continue;
            }
            if end.y >= start.y && other.y >= start.y - EPS {
                y = y.min(other.y);
            } else if end.y < start.y && other.y + other.height <= start.y + EPS {
                y = y.max(other.y + other.height);
            } else {
                valid = false;
                break;
            }
        }
        let candidate = bounds(start, pos2(x, y));
        if valid
            && candidate.width > EPS
            && candidate.height > EPS
            && candidate.valid()
            && !others.iter().any(|other| overlaps(candidate, *other))
            && best.is_none_or(|old| candidate.width * candidate.height > old.width * old.height)
        {
            best = Some(candidate);
        }
    }
    best
}

pub fn move_zone(mut wanted: Bounds, others: &[Bounds], size: Vec2) -> Option<Bounds> {
    wanted.x = wanted.x.clamp(0.0, (1.0 - wanted.width).max(0.0));
    wanted.y = wanted.y.clamp(0.0, (1.0 - wanted.height).max(0.0));
    let (x, y) = lines(others);
    let move_axis = |value: f32, length: f32, lines: &[f32], extent: f32| {
        let positions: Vec<_> = lines
            .iter()
            .flat_map(|line| [*line, *line - length])
            .filter(|v| *v >= 0.0 && *v <= 1.0 - length + EPS)
            .collect();
        snap(value, &positions, extent).clamp(0.0, (1.0 - length).max(0.0))
    };
    wanted.x = move_axis(wanted.x, wanted.width, &x, size.x);
    wanted.y = move_axis(wanted.y, wanted.height, &y, size.y);
    if !others.iter().any(|other| overlaps(wanted, *other)) {
        return Some(wanted);
    }
    let mut xs = vec![wanted.x, 0.0, 1.0 - wanted.width];
    let mut ys = vec![wanted.y, 0.0, 1.0 - wanted.height];
    for other in others {
        xs.extend([other.x - wanted.width, other.x + other.width]);
        ys.extend([other.y - wanted.height, other.y + other.height]);
    }
    let mut best = None;
    let mut distance = f32::INFINITY;
    for x in xs {
        for &y in &ys {
            let candidate = Bounds { x, y, ..wanted };
            if !candidate.valid() || others.iter().any(|other| overlaps(candidate, *other)) {
                continue;
            }
            let next = ((x - wanted.x) * size.x).powi(2) + ((y - wanted.y) * size.y).powi(2);
            if next < distance {
                distance = next;
                best = Some(candidate);
            }
        }
    }
    best
}

pub fn align(zones: &[Bounds], size: Vec2) -> Vec<Bounds> {
    let mut result = Vec::new();
    for original in zones {
        let start = point(pos2(original.x, original.y), &[], size);
        let end = point(
            pos2(original.x + original.width, original.y + original.height),
            &[],
            size,
        );
        let snapped = bounds(start, end);
        let candidate = if snapped.width > EPS && snapped.height > EPS {
            snapped
        } else {
            *original
        };
        let fitted = move_zone(candidate, &result, size)
            .or_else(|| {
                fit(
                    pos2(candidate.x, candidate.y),
                    pos2(
                        candidate.x + candidate.width,
                        candidate.y + candidate.height,
                    ),
                    &result,
                )
            })
            .unwrap_or(*original);
        result.push(fitted);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }
    #[test]
    fn drawing_docks_to_grid_and_stops_at_neighbour_in_both_directions() {
        let size = Vec2::new(200.0, 120.0);
        let left = draw(pos2(0.01, 0.01), pos2(0.526, 0.99), &[], size).unwrap();
        assert_eq!(
            (left.x, left.y, left.width, left.height),
            (0.0, 0.0, 0.5, 1.0)
        );
        let right = draw(pos2(0.99, 0.99), pos2(0.4, 0.01), &[left], size).unwrap();
        assert_eq!((right.x, right.width), (0.5, 0.5));
        assert!(!overlaps(left, right));
        assert!(draw(pos2(0.2, 0.2), pos2(0.8, 0.8), &[left], size).is_none());
    }
    #[test]
    fn move_preserves_size_and_resize_preserves_origin_without_overlap() {
        let obstacle = rect(0.5, 0.0, 0.5, 1.0);
        let moved = move_zone(
            rect(0.4, 0.1, 0.3, 0.7),
            &[obstacle],
            Vec2::new(400.0, 300.0),
        )
        .unwrap();
        assert!((moved.x - 0.2).abs() < EPS);
        assert_eq!((moved.width, moved.height), (0.3, 0.7));
        let resized = resize(
            rect(0.0, 0.0, 0.2, 1.0),
            pos2(0.7, 1.0),
            &[obstacle],
            Vec2::splat(400.0),
        )
        .unwrap();
        assert_eq!((resized.x, resized.y, resized.width), (0.0, 0.0, 0.5));
        assert!(!overlaps(resized, obstacle));
    }
    #[test]
    fn existing_imprecise_halves_align_and_share_pixel_edge() {
        let result = align(
            &[
                rect(0.0, 0.0, 0.52616876, 1.0),
                rect(0.50520825, 0.0, 0.49479175, 1.0),
            ],
            Vec2::new(205.0, 130.0),
        );
        assert_eq!(
            (result[0].width, result[1].x, result[1].width),
            (0.5, 0.5, 0.5)
        );
        assert!(!overlaps(result[0], result[1]));
        for width in [1366, 1920, 2560] {
            let monitor = crate::core::monitor::Monitor {
                name: "DP-1".into(),
                x: -width,
                y: -200,
                width: width as u32,
                height: 1080,
                scale: 1.5,
            };
            let a = result[0].pixels(&monitor);
            let b = result[1].pixels(&monitor);
            assert_eq!(a.0 + a.2 as i32, b.0);
        }
    }
}
