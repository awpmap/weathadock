//! What is behind the desktop widget: how bright the wallpaper is there, so the widget can
//! choose dark or light text by itself. Only geometry and arithmetic live here; asking Windows
//! for the wallpaper is the app's job.

/// How Windows lays a picture over a screen ("Choose a fit" in Settings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// At its own size, in the middle.
    Center,
    /// At its own size, repeated from the top left.
    Tile,
    /// Squeezed or stretched to the screen's exact shape.
    Stretch,
    /// As large as fits whole; the desktop colour shows beside it.
    Fit,
    /// Large enough to cover the screen; what sticks out is cut off.
    Fill,
    /// Like `Fill`, across all screens together.
    Span,
}

/// A rectangle of the desktop, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Area {
    fn width(&self) -> f64 {
        self.right - self.left
    }
    fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

/// The wallpaper picture, shrunk to a few thousand pixels: `pixels` holds `width * height`
/// values of four bytes each (red, green, blue and one unused), row by row. `natural` is the
/// size of the picture file itself, which is what the fit works from.
#[derive(Clone, Copy, Debug)]
pub struct Picture<'a> {
    pub pixels: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub natural: (f64, f64),
}

/// How light a colour looks, 0 (black) to 1 (white).
fn lightness(rgb: [u8; 3]) -> f64 {
    (0.2126 * rgb[0] as f64 + 0.7152 * rgb[1] as f64 + 0.0722 * rgb[2] as f64) / 255.0
}

/// How light the desktop looks behind `area`, 0 (black) to 1 (white).
///
/// `screen` is the monitor the area is on (all monitors together for `Fit::Span`), and
/// `colour` is the desktop's background colour: all there is without a picture, and what shows
/// around a picture that does not reach the edges.
pub fn brightness_behind(picture: Option<&Picture>, fit: Fit, colour: [u8; 3], screen: Area, area: Area) -> f64 {
    let usable = |p: &&Picture| {
        p.width > 0 && p.height > 0 && p.pixels.len() >= p.width * p.height * 4 && p.natural.0 >= 1.0 && p.natural.1 >= 1.0
    };
    let sized = screen.width() >= 1.0 && screen.height() >= 1.0 && area.width() > 0.0 && area.height() > 0.0;
    let Some(p) = picture.filter(usable).filter(|_| sized) else { return lightness(colour) };
    let (wide, high) = p.natural;
    // The picture's scale on the screen, and where its top left corner lands.
    let (kx, ky) = match fit {
        Fit::Center | Fit::Tile => (1.0, 1.0),
        Fit::Stretch => (screen.width() / wide, screen.height() / high),
        Fit::Fit => {
            let k = (screen.width() / wide).min(screen.height() / high);
            (k, k)
        }
        Fit::Fill | Fit::Span => {
            let k = (screen.width() / wide).max(screen.height() / high);
            (k, k)
        }
    };
    let (x0, y0) = match fit {
        Fit::Tile => (screen.left, screen.top),
        _ => (screen.left + (screen.width() - wide * kx) / 2.0, screen.top + (screen.height() - high * ky) / 2.0),
    };
    const ACROSS: usize = 24;
    const DOWN: usize = 16;
    let mut sum = 0.0;
    for row in 0..DOWN {
        for col in 0..ACROSS {
            let x = area.left + area.width() * (col as f64 + 0.5) / ACROSS as f64;
            let y = area.top + area.height() * (row as f64 + 0.5) / DOWN as f64;
            let (mut u, mut v) = ((x - x0) / kx, (y - y0) / ky);
            if fit == Fit::Tile {
                (u, v) = (u.rem_euclid(wide), v.rem_euclid(high));
            }
            sum += if (0.0..wide).contains(&u) && (0.0..high).contains(&v) {
                let px = ((u / wide * p.width as f64) as usize).min(p.width - 1);
                let py = ((v / high * p.height as f64) as usize).min(p.height - 1);
                let i = (py * p.width + px) * 4;
                lightness([p.pixels[i], p.pixels[i + 1], p.pixels[i + 2]])
            } else {
                lightness(colour)
            };
        }
    }
    sum / (ACROSS * DOWN) as f64
}

/// Whether the widget should use dark text: yes once the desktop behind it is on the bright
/// side, no once it is on the dark side, and whatever it uses now in the narrow band between,
/// so that a wallpaper right at the middle does not make the text flip back and forth each
/// time the widget changes size by a few pixels.
///
/// The middle is where it is for two reasons. On an even backdrop black and white text stand
/// out equally at a lightness of 0.46, and above that black wins quickly: on a mid blue sky of
/// 0.58, white text has a contrast of 2.9 to 1 and black text 7.3 to 1. And the light Windows
/// 11 wallpaper measures 0.45 behind a widget in the top right corner, pale at the top and
/// deep blue below, where light text is the better choice by far.
pub fn dark_text(brightness: f64, dark_now: bool) -> bool {
    if brightness >= 0.53 {
        true
    } else if brightness <= 0.47 {
        false
    } else {
        dark_now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area { left: 0.0, top: 0.0, right: 2560.0, bottom: 1440.0 };
    const GREY: [u8; 3] = [128, 128, 128];

    /// A picture whose left half is white and whose right half is black.
    fn halves(width: usize, height: usize) -> Vec<u8> {
        (0..width * height).flat_map(|i| if i % width < width / 2 { [255, 255, 255, 0] } else { [0, 0, 0, 0] }).collect()
    }
    fn at(left: f64, top: f64, wide: f64, high: f64) -> Area {
        Area { left, top, right: left + wide, bottom: top + high }
    }
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.02
    }

    #[test]
    fn without_a_picture_the_desktop_colour_is_behind_everything() {
        assert!(close(brightness_behind(None, Fit::Fill, [255, 255, 255], SCREEN, at(100.0, 100.0, 380.0, 420.0)), 1.0));
        assert!(close(brightness_behind(None, Fit::Fill, [0, 0, 0], SCREEN, at(100.0, 100.0, 380.0, 420.0)), 0.0));
        // Green looks lighter than blue at the same strength.
        assert!(brightness_behind(None, Fit::Fill, [0, 200, 0], SCREEN, SCREEN) > brightness_behind(None, Fit::Fill, [0, 0, 200], SCREEN, SCREEN) + 0.4);
    }

    #[test]
    fn a_filling_picture_is_read_where_the_widget_sits() {
        let pixels = halves(64, 36);
        let p = Picture { pixels: &pixels, width: 64, height: 36, natural: (3840.0, 2160.0) };
        // Left half of the screen: white behind. Right half: black. Across the middle: half and half.
        assert!(close(brightness_behind(Some(&p), Fit::Fill, GREY, SCREEN, at(40.0, 40.0, 380.0, 420.0)), 1.0));
        assert!(close(brightness_behind(Some(&p), Fit::Fill, GREY, SCREEN, at(2140.0, 40.0, 380.0, 420.0)), 0.0));
        assert!(close(brightness_behind(Some(&p), Fit::Fill, GREY, SCREEN, at(1280.0 - 190.0, 500.0, 380.0, 420.0)), 0.5));
        // Stretching gives the same answer for a picture of the screen's own shape.
        assert!(close(brightness_behind(Some(&p), Fit::Stretch, GREY, SCREEN, at(40.0, 40.0, 380.0, 420.0)), 1.0));
    }

    #[test]
    fn a_picture_wider_than_the_screen_loses_its_sides_when_it_fills() {
        // Twice as wide as the screen's shape: filling shows only its middle half, so the
        // white-to-black edge is still mid-screen, but 300 px in from the left is still white.
        let pixels = halves(128, 36);
        let p = Picture { pixels: &pixels, width: 128, height: 36, natural: (5120.0, 1440.0) };
        assert!(close(brightness_behind(Some(&p), Fit::Fill, GREY, SCREEN, at(300.0, 500.0, 300.0, 300.0)), 1.0));
        assert!(close(brightness_behind(Some(&p), Fit::Fill, GREY, SCREEN, at(1400.0, 500.0, 300.0, 300.0)), 0.0));
        // Fitted whole, it is a band across the middle with the desktop colour above and below.
        assert!(close(brightness_behind(Some(&p), Fit::Fit, [255, 0, 0], SCREEN, at(300.0, 20.0, 300.0, 200.0)), lightness([255, 0, 0])));
        assert!(close(brightness_behind(Some(&p), Fit::Fit, [255, 0, 0], SCREEN, at(300.0, 600.0, 300.0, 200.0)), 1.0));
        // Stretched, the whole picture is squeezed onto the screen.
        assert!(close(brightness_behind(Some(&p), Fit::Stretch, GREY, SCREEN, at(1000.0, 20.0, 200.0, 200.0)), 1.0));
        assert!(close(brightness_behind(Some(&p), Fit::Stretch, GREY, SCREEN, at(1400.0, 20.0, 200.0, 200.0)), 0.0));
    }

    #[test]
    fn a_small_picture_sits_in_the_middle_or_repeats() {
        let pixels = halves(40, 40);
        let p = Picture { pixels: &pixels, width: 40, height: 40, natural: (400.0, 400.0) };
        // Centred: 400 px of picture in the middle of the screen, desktop colour everywhere else.
        assert!(close(brightness_behind(Some(&p), Fit::Center, [0, 0, 0], SCREEN, at(40.0, 40.0, 300.0, 300.0)), 0.0));
        assert!(close(brightness_behind(Some(&p), Fit::Center, [0, 0, 0], SCREEN, at(1080.0, 520.0, 200.0, 400.0)), 1.0));
        assert!(close(brightness_behind(Some(&p), Fit::Center, [0, 0, 0], SCREEN, at(1280.0, 520.0, 200.0, 400.0)), 0.0));
        // Tiled: white and black stripes 200 px wide, from the top left corner.
        assert!(close(brightness_behind(Some(&p), Fit::Tile, GREY, SCREEN, at(800.0, 800.0, 200.0, 100.0)), 1.0));
        assert!(close(brightness_behind(Some(&p), Fit::Tile, GREY, SCREEN, at(1000.0, 800.0, 200.0, 100.0)), 0.0));
        assert!(close(brightness_behind(Some(&p), Fit::Tile, GREY, SCREEN, at(0.0, 0.0, 2400.0, 400.0)), 0.5));
    }

    #[test]
    fn a_second_screen_is_measured_from_its_own_corner() {
        let pixels = halves(64, 36);
        let p = Picture { pixels: &pixels, width: 64, height: 36, natural: (1920.0, 1080.0) };
        let left_screen = Area { left: -1920.0, top: 0.0, right: 0.0, bottom: 1080.0 };
        assert!(close(brightness_behind(Some(&p), Fit::Fill, GREY, left_screen, at(-1900.0, 40.0, 380.0, 420.0)), 1.0));
        assert!(close(brightness_behind(Some(&p), Fit::Fill, GREY, left_screen, at(-400.0, 40.0, 380.0, 420.0)), 0.0));
        // One picture spanning both screens: the white half covers the left screen.
        let both = Area { left: -1920.0, top: 0.0, right: 1920.0, bottom: 1080.0 };
        let wide = Picture { natural: (3840.0, 1080.0), ..p };
        assert!(close(brightness_behind(Some(&wide), Fit::Span, GREY, both, at(-400.0, 40.0, 380.0, 420.0)), 1.0));
        assert!(close(brightness_behind(Some(&wide), Fit::Span, GREY, both, at(20.0, 40.0, 380.0, 420.0)), 0.0));
    }

    #[test]
    fn nothing_odd_can_break_the_measurement() {
        let pixels = halves(8, 8);
        let short = Picture { pixels: &pixels[..40], width: 8, height: 8, natural: (800.0, 800.0) };
        let flat = Picture { pixels: &pixels, width: 8, height: 8, natural: (0.0, f64::NAN) };
        let empty = Picture { pixels: &[], width: 0, height: 0, natural: (800.0, 800.0) };
        let good = Picture { pixels: &pixels, width: 8, height: 8, natural: (800.0, 800.0) };
        let nowhere = Area { left: 5.0, top: 5.0, right: 5.0, bottom: 5.0 };
        let far = at(1e12, -1e12, 400.0, 400.0);
        for fit in [Fit::Center, Fit::Tile, Fit::Stretch, Fit::Fit, Fit::Fill, Fit::Span] {
            for p in [&short, &flat, &empty] {
                assert!(close(brightness_behind(Some(p), fit, GREY, SCREEN, at(0.0, 0.0, 300.0, 300.0)), lightness(GREY)));
            }
            for (screen, area) in [(SCREEN, nowhere), (nowhere, SCREEN), (SCREEN, far), (Area { left: f64::NAN, ..SCREEN }, SCREEN)] {
                let b = brightness_behind(Some(&good), fit, GREY, screen, area);
                assert!((0.0..=1.0).contains(&b), "{fit:?} {b}");
            }
        }
    }

    #[test]
    fn the_text_changes_only_when_the_wallpaper_is_clearly_bright_or_clearly_not() {
        assert!(dark_text(0.8, false) && dark_text(0.53, false));
        assert!(!dark_text(0.2, true) && !dark_text(0.47, true));
        // The light Windows 11 wallpaper (0.45) and a mid blue sky (0.58), whatever came before.
        assert!(!dark_text(0.45, true) && dark_text(0.58, false));
        // In between it stays as it is, whichever that is.
        assert!(dark_text(0.5, true) && !dark_text(0.5, false));
        assert!(!dark_text(f64::NAN, false));
    }
}
