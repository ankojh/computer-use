//! Coordinate mapping between model image space `I`, global points `P`, and device pixels `D`.
//! See HANDOFF.md §6.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::action::{PointD, PointI, PointP, RectI, RectP};

#[derive(Debug, Error, PartialEq)]
pub enum CoordError {
    #[error("coordinate ({x}, {y}) is outside the {w}×{h} screenshot")]
    OutOfBounds { x: f64, y: f64, w: u32, h: u32 },
    #[error("invalid region: {0}")]
    InvalidRegion(String),
    #[error("coordinate is not a finite number")]
    NotFinite,
}

/// The exact geometry of one screenshot: which display region it covers and at what size it was sent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CoordMapping {
    /// Region of the display (global points) covered by the image. The full display bounds `B`
    /// for normal screenshots.
    pub source: RectP,
    /// Main display bounds in global points.
    pub display_bounds: RectP,
    /// Image size `Wi × Hi` in pixels.
    pub image_w: u32,
    pub image_h: u32,
    /// Device pixels per point.
    pub scale: f64,
    pub display_generation: u64,
    pub frame_seq: u64,
}

/// Tolerance for coordinates that land exactly on (or a hair past) the image edge.
const EDGE_EPS: f64 = 0.5;

impl CoordMapping {
    /// Mapping for a full-display screenshot of `display_bounds` sent at `image_w × image_h`.
    pub fn full_display(display_bounds: RectP, image_w: u32, image_h: u32, scale: f64) -> Self {
        CoordMapping {
            source: display_bounds,
            display_bounds,
            image_w,
            image_h,
            scale,
            display_generation: 0,
            frame_seq: 0,
        }
    }

    fn sx(&self) -> f64 {
        self.source.w / self.image_w as f64
    }

    fn sy(&self) -> f64 {
        self.source.h / self.image_h as f64
    }

    /// Image → points (for clicks). Out-of-image coordinates are an error returned to the model,
    /// never silently clamped.
    pub fn image_to_points(&self, p: PointI) -> Result<PointP, CoordError> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(CoordError::NotFinite);
        }
        let (w, h) = (self.image_w as f64, self.image_h as f64);
        if p.x < -EDGE_EPS || p.y < -EDGE_EPS || p.x > w + EDGE_EPS || p.y > h + EDGE_EPS {
            return Err(CoordError::OutOfBounds {
                x: p.x,
                y: p.y,
                w: self.image_w,
                h: self.image_h,
            });
        }
        let x = self.source.x + p.x.clamp(0.0, w) * self.sx();
        let y = self.source.y + p.y.clamp(0.0, h) * self.sy();
        // Keep the point on the display: x == B.x + B.w is one past the last column.
        let b = &self.display_bounds;
        Ok(PointP {
            x: x.clamp(b.x, b.x + b.w - 0.01),
            y: y.clamp(b.y, b.y + b.h - 0.01),
        })
    }

    /// Points → image (for reporting cursor position or AX frames to the model).
    pub fn points_to_image(&self, p: PointP) -> PointI {
        PointI {
            x: (p.x - self.source.x) / self.sx(),
            y: (p.y - self.source.y) / self.sy(),
        }
    }

    /// Points → device pixels, relative to the display framebuffer origin.
    pub fn points_to_device(&self, p: PointP) -> PointD {
        PointD {
            x: (p.x - self.display_bounds.x) * self.scale,
            y: (p.y - self.display_bounds.y) * self.scale,
        }
    }

    /// A length in image pixels → points (for pixel scroll deltas).
    pub fn image_delta_to_points(&self, dx: f64, dy: f64) -> (f64, f64) {
        (dx * self.sx(), dy * self.sy())
    }

    /// Image-space region (corners) → points rect, e.g. for `zoom`.
    pub fn image_rect_to_points(&self, r: RectI) -> Result<RectP, CoordError> {
        if !(r.x1 > r.x0 && r.y1 > r.y0) {
            return Err(CoordError::InvalidRegion(format!(
                "[{}, {}, {}, {}] must have x1 > x0 and y1 > y0",
                r.x0, r.y0, r.x1, r.y1
            )));
        }
        let a = self.image_to_points(PointI { x: r.x0, y: r.y0 })?;
        let b = self.image_to_points(PointI { x: r.x1, y: r.y1 })?;
        Ok(RectP {
            x: a.x,
            y: a.y,
            w: b.x - a.x,
            h: b.y - a.y,
        })
    }
}

/// Fits `w × h` inside `max_long_edge` preserving aspect ratio (same rule as the Swift side).
pub fn fit_size(w: u32, h: u32, max_long_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if max_long_edge == 0 || long <= max_long_edge {
        return (w, h);
    }
    let s = max_long_edge as f64 / long as f64;
    if w >= h {
        (max_long_edge, ((h as f64 * s).round() as u32).max(1))
    } else {
        (((w as f64 * s).round() as u32).max(1), max_long_edge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn this_machine() -> CoordMapping {
        let (wi, hi) = fit_size(1728, 1117, 1440);
        CoordMapping::full_display(
            RectP {
                x: 0.0,
                y: 0.0,
                w: 1728.0,
                h: 1117.0,
            },
            wi,
            hi,
            2.0,
        )
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn worked_example_from_handoff() {
        let m = this_machine();
        assert_eq!((m.image_w, m.image_h), (1440, 931));
        let p = m.image_to_points(PointI { x: 720.0, y: 465.5 }).unwrap();
        assert!(close(p.x, 864.0, 1e-9), "{p:?}");
        assert!(close(p.y, 558.5, 0.1), "{p:?}");
        let d = m.points_to_device(p);
        assert!(close(d.x, 1728.0, 1e-9), "{d:?}");
        assert!(close(d.y, 1117.0, 0.2), "{d:?}");
    }

    #[test]
    fn out_of_bounds_is_error() {
        let m = this_machine();
        assert!(matches!(
            m.image_to_points(PointI { x: 1441.0, y: 10.0 }),
            Err(CoordError::OutOfBounds { .. })
        ));
        assert!(matches!(
            m.image_to_points(PointI { x: 10.0, y: -1.0 }),
            Err(CoordError::OutOfBounds { .. })
        ));
        assert_eq!(
            m.image_to_points(PointI {
                x: f64::NAN,
                y: 0.0
            }),
            Err(CoordError::NotFinite)
        );
        // The exact edge is accepted and kept on the display.
        let p = m
            .image_to_points(PointI {
                x: 1440.0,
                y: 931.0,
            })
            .unwrap();
        assert!(p.x < 1728.0 && p.y < 1117.0);
    }

    #[test]
    fn non_zero_origin_and_crop() {
        // A display whose bounds start at (-1440, 200) and an image that covers a crop of it.
        let bounds = RectP {
            x: -1440.0,
            y: 200.0,
            w: 1440.0,
            h: 900.0,
        };
        let mut m = CoordMapping::full_display(bounds, 720, 450, 1.0);
        let p = m.image_to_points(PointI { x: 360.0, y: 225.0 }).unwrap();
        assert_eq!(
            p,
            PointP {
                x: -720.0,
                y: 650.0
            }
        );
        m.source = RectP {
            x: -1000.0,
            y: 300.0,
            w: 200.0,
            h: 100.0,
        };
        m.image_w = 400;
        m.image_h = 200;
        let p = m.image_to_points(PointI { x: 100.0, y: 50.0 }).unwrap();
        assert_eq!(
            p,
            PointP {
                x: -950.0,
                y: 325.0
            }
        );
        let d = m.points_to_device(p);
        assert_eq!(d, PointD { x: 490.0, y: 125.0 });
    }

    #[test]
    fn zoom_region() {
        let m = this_machine();
        let r = m
            .image_rect_to_points(RectI {
                x0: 0.0,
                y0: 0.0,
                x1: 720.0,
                y1: 465.5,
            })
            .unwrap();
        assert!(close(r.w, 864.0, 1e-9) && close(r.h, 558.5, 0.1), "{r:?}");
        assert!(m
            .image_rect_to_points(RectI {
                x0: 10.0,
                y0: 0.0,
                x1: 5.0,
                y1: 10.0
            })
            .is_err());
    }

    #[test]
    fn fit_size_rules() {
        assert_eq!(fit_size(3456, 2234, 1440), (1440, 931));
        assert_eq!(fit_size(2234, 3456, 1440), (931, 1440));
        assert_eq!(fit_size(800, 600, 1440), (800, 600));
        assert_eq!(fit_size(800, 600, 0), (800, 600));
    }

    proptest! {
        #[test]
        fn image_points_roundtrip(
            bx in -3000.0f64..3000.0, by in -3000.0f64..3000.0,
            bw in 640.0f64..6000.0, bh in 480.0f64..4000.0,
            scale in prop::sample::select(vec![1.0f64, 2.0, 3.0]),
            max_edge in 640u32..2600,
            fx in 0.0f64..1.0, fy in 0.0f64..1.0,
        ) {
            let bounds = RectP { x: bx, y: by, w: bw.round(), h: bh.round() };
            let (wi, hi) = fit_size((bounds.w * scale) as u32, (bounds.h * scale) as u32, max_edge);
            let m = CoordMapping::full_display(bounds, wi, hi, scale);
            let i = PointI { x: fx * (wi as f64 - 1.0), y: fy * (hi as f64 - 1.0) };
            let p = m.image_to_points(i).unwrap();
            prop_assert!(bounds.contains(p));
            let back = m.points_to_image(p);
            prop_assert!(close(back.x, i.x, 1e-6) && close(back.y, i.y, 1e-6));
            let d = m.points_to_device(p);
            prop_assert!(close(d.x, (p.x - bx) * scale, 1e-6));
        }
    }
}
