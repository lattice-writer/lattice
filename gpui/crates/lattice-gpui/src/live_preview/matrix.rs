//! The affine maps the live preview's painter draws with, from sp on the
//! page to device pixels (`paint.rs`).

use tiny_skia::Transform;

/// Affine map [a, b, c, d, e, f]: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat(pub [f64; 6]);

impl Mat {
    pub const IDENTITY: Mat = Mat([1., 0., 0., 1., 0., 0.]);

    pub fn scale(k: f64) -> Mat {
        Mat([k, 0., 0., k, 0., 0.])
    }

    /// `self` after `other`: `other` applies first.
    pub fn mul(self, other: Mat) -> Mat {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = other.0;
        Mat([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }

    pub(super) fn skia(self) -> Transform {
        let [a, b, c, d, e, f] = self.0.map(|v| v as f32);
        Transform::from_row(a, b, c, d, e, f)
    }

    pub(super) fn apply(self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_compose_with_the_right_one_first() {
        let scale = Mat::scale(2.);
        let shift = Mat([1., 0., 0., 1., 10., 0.]);
        assert_eq!(scale.mul(shift).apply(1., 1.), (22., 2.));
        assert_eq!(shift.mul(scale).apply(1., 1.), (12., 2.));
    }
}
