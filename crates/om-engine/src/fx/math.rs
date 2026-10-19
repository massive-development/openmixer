//! Realtime helpers shared by the FX.

/// Tiny feedback values become denormals on some CPUs; snapping them keeps the cost flat.
#[inline]
pub fn flush_denormal(x: f32) -> f32 {
    if x.abs() < 1.0e-15 {
        0.0
    } else {
        x
    }
}
