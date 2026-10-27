#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiDensity {
    pub compact: bool,
    pub narrow: bool,
    pub page_inset: u32,
    pub page_bottom_inset: u32,
    pub page_gap: u32,
    pub header_button_size: u32,
    pub surface_title_size: u32,
    pub surface_subtitle_size: u32,
    pub surface_button_height: u32,
}

impl UiDensity {
    pub fn for_window(window_width: u32, is_android: bool) -> Self {
        let compact = is_android || window_width <= 1000;
        let narrow = is_android || window_width <= 640;
        Self {
            // Android handhelds report their physical pixel width here. Treat them as
            // compact even when that number is larger than a desktop window width.
            compact,
            narrow,
            page_inset: if is_android {
                4
            } else if compact {
                14
            } else {
                24
            },
            page_bottom_inset: if is_android {
                10
            } else if compact {
                30
            } else {
                40
            },
            page_gap: if is_android { 6 } else { 12 },
            header_button_size: if is_android { 32 } else { 42 },
            surface_title_size: if is_android {
                17
            } else if narrow {
                19
            } else {
                25
            },
            surface_subtitle_size: if is_android {
                10
            } else if narrow {
                11
            } else {
                13
            },
            surface_button_height: if is_android { 36 } else { 52 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UiDensity;

    #[test]
    fn android_uses_handheld_surface_metrics_at_retroid_pixel_width() {
        let density = UiDensity::for_window(1920, true);

        assert!(density.compact);
        assert!(density.narrow);
        assert_eq!(density.page_inset, 4);
        assert_eq!(density.page_bottom_inset, 10);
        assert_eq!(density.page_gap, 6);
        assert_eq!(density.header_button_size, 32);
        assert_eq!(density.surface_title_size, 17);
        assert_eq!(density.surface_subtitle_size, 10);
        assert_eq!(density.surface_button_height, 36);
    }

    #[test]
    fn desktop_window_keeps_existing_wide_metrics() {
        let density = UiDensity::for_window(1920, false);

        assert!(!density.compact);
        assert!(!density.narrow);
        assert_eq!(density.page_inset, 24);
        assert_eq!(density.page_bottom_inset, 40);
        assert_eq!(density.header_button_size, 42);
        assert_eq!(density.surface_title_size, 25);
        assert_eq!(density.surface_subtitle_size, 13);
        assert_eq!(density.surface_button_height, 52);
    }
}
