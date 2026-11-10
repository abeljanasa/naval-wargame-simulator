/// Normalises a longitude into the half-open range −180..180 degrees.
#[must_use]
pub fn normalize_lon_deg(lon: f64) -> f64 {
    let mut x = lon % 360.0;
    if x >= 180.0 {
        x -= 360.0;
    } else if x < -180.0 {
        x += 360.0;
    }
    x
}

/// Normalises an azimuth into the half-open range 0..360 degrees (clockwise from true north).
#[must_use]
pub fn normalize_azimuth_deg(az: f64) -> f64 {
    let x = az % 360.0;
    if x < 0.0 {
        x + 360.0
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lon_wraps() {
        assert_eq!(normalize_lon_deg(180.0), -180.0);
        assert_eq!(normalize_lon_deg(190.0), -170.0);
        assert_eq!(normalize_lon_deg(-190.0), 170.0);
        assert_eq!(normalize_lon_deg(540.0), -180.0);
        assert_eq!(normalize_lon_deg(12.5), 12.5);
    }

    #[test]
    fn azimuth_wraps() {
        assert_eq!(normalize_azimuth_deg(-90.0), 270.0);
        assert_eq!(normalize_azimuth_deg(360.0), 0.0);
        assert_eq!(normalize_azimuth_deg(725.0), 5.0);
    }
}
