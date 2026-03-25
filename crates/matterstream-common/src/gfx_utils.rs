//! Common graphics math — color packing, unpacking. Backend-agnostic.

/// Pack RGBA components into a u32 (0xRRGGBBAA).
pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32
}

/// Unpack a u32 RGBA (0xRRGGBBAA) into (r, g, b, a).
pub fn rgba_unpack(c: u32) -> (u8, u8, u8, u8) {
    ((c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_pack_unpack_roundtrip() {
        let (r, g, b, a) = (0x12, 0x34, 0x56, 0x78);
        let packed = rgba(r, g, b, a);
        assert_eq!(packed, 0x12345678);
        let (ur, ug, ub, ua) = rgba_unpack(packed);
        assert_eq!((ur, ug, ub, ua), (r, g, b, a));
    }

    #[test]
    fn rgba_zero() {
        assert_eq!(rgba(0, 0, 0, 0), 0);
        assert_eq!(rgba_unpack(0), (0, 0, 0, 0));
    }

    #[test]
    fn rgba_max() {
        assert_eq!(rgba(255, 255, 255, 255), 0xFFFFFFFF);
        assert_eq!(rgba_unpack(0xFFFFFFFF), (255, 255, 255, 255));
    }
}
