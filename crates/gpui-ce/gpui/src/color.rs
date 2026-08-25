use palette::{OklabHue, Oklcha, RgbHue};
use schemars::{JsonSchema, json_schema};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

// Re-exported for api maintenance
pub use palette::{Hsla, IntoColor, WithAlpha, rgb::Rgba};

/// Convert an RGB hex color code number to a color type
pub fn rgb(hex: u32) -> Rgba {
    let [_, r, g, b] = hex.to_be_bytes().map(|b| (b as f32) / 255.0);
    Rgba {
        color: palette::rgb::Rgb::new(r, g, b),
        alpha: 1.0,
    }
}

/// Convert an RGBA hex color code number to [`Rgba`]
pub fn rgba(hex: u32) -> Rgba {
    let [r, g, b, a] = hex.to_be_bytes().map(|b| (b as f32) / 255.0);
    Rgba::new(r, g, b, a)
}

/// Swap from RGBA with premultiplied alpha to BGRA
pub fn swap_rgba_pa_to_bgra(color: &mut [u8]) {
    color.swap(0, 2);
    if color[3] > 0 {
        let a = color[3] as f32 / 255.;
        color[0] = (color[0] as f32 / a) as u8;
        color[1] = (color[1] as f32 / a) as u8;
        color[2] = (color[2] as f32 / a) as u8;
    }
}

/// Construct an [`Hsla`] object from plain values
pub const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla {
        color: palette::Hsl::new_const(
            // `RgbHue` stores degrees, so the 0..1 fraction of the circle needs
            // scaling to 0..360 before it's wrapped.
            RgbHue::new(h.clamp(0., 1.) * 360.),
            s.clamp(0., 1.),
            l.clamp(0., 1.),
        ),
        alpha: a.clamp(0., 1.),
    }
}

/// Constructs a ['Oklcha'](palette::Oklcha) object from plain values.
pub fn oklcha<T>(lightness: T, chroma: T, hue: impl Into<OklabHue<T>>, alpha: T) -> Oklcha<T> {
    Oklcha::new(lightness, chroma, hue, alpha)
}

/// Pure black in [`Hsla`]
pub const fn black() -> Hsla {
    Hsla::new_const(RgbHue::new(0.), 0., 0., 1.)
}

/// Transparent black in [`Hsla`]
pub const fn transparent_black() -> Hsla {
    Hsla::new_const(RgbHue::new(0.), 0., 0., 0.)
}

/// Transparent white in [`Hsla`]
pub const fn transparent_white() -> Hsla {
    Hsla::new_const(RgbHue::new(0.), 0., 1., 0.)
}

/// Opaque grey in [`Hsla`], values must be provided in the range [0, 1]
pub const fn opaque_grey(lightness: f32, opacity: f32) -> Hsla {
    Hsla::new_const(RgbHue::new(0.), 0., lightness, opacity)
}

/// Pure white in [`Hsla`]
pub const fn white() -> Hsla {
    Hsla::new_const(RgbHue::new(0.), 0., 1., 1.)
}

/// The color red in [`Hsla`]
pub const fn red() -> Hsla {
    Hsla::new_const(RgbHue::new(0.), 1., 0.5, 1.)
}

/// The color blue in [`Hsla`]
pub const fn blue() -> Hsla {
    Hsla::new_const(RgbHue::new(240.), 1., 0.5, 1.)
}

/// The color green in [`Hsla`]
pub const fn green() -> Hsla {
    Hsla::new_const(RgbHue::new(120.), 1., 0.25, 1.)
}

/// The color yellow in [`Hsla`]
pub const fn yellow() -> Hsla {
    Hsla::new_const(RgbHue::new(60.), 1., 0.5, 1.)
}

/// Generates the JsonSchema for palette::Hsla
pub fn hsla_schemar(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    // https://github.com/Ogeon/palette/blob/9aa1ac21a7da60db398e8c044a43dbf3fdaf4855/palette/src/hsl.rs#L633-L636
    // https://github.com/Ogeon/palette/blob/9aa1ac21a7da60db398e8c044a43dbf3fdaf4855/palette/src/alpha/alpha.rs#L1197
    json_schema!({
        "type": "object",
        "properties": {
          "hue": {
              "type": "number",
              "format": "float"
          },
          "saturation": {
              "type": "number",
              "format": "float"
          },
          "lightness": {
              "type": "number",
              "format": "float"
          },
          "alpha": {
              "type": "number",
              "format": "float"
          },
        }
    })
}

/// Generates the JsonSchema for palette::Rgba
pub fn rgba_schemar(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    // https://github.com/Ogeon/palette/blob/9aa1ac21a7da60db398e8c044a43dbf3fdaf4855/palette/src/rgb/rgb.rs#L1690
    // https://github.com/Ogeon/palette/blob/9aa1ac21a7da60db398e8c044a43dbf3fdaf4855/palette/src/alpha/alpha.rs#L1197
    json_schema!({
        "type": "object",
        "properties": {
          "red": {
              "type": "number",
              "format": "float"
          },
          "green": {
              "type": "number",
              "format": "float"
          },
          "blue": {
              "type": "number",
              "format": "float"
          },
          "alpha": {
              "type": "number",
              "format": "float"
          },
        }
    })
}

/// Wrapper methods to make alpha operations more convenient
pub trait ColorExt {
    /// Performs a SrcAlpha x (1 - SrcAlpha) blend
    fn blend(&self, other: &Self) -> Self
    where
        Self: Sized;

    /// Fade out the color by a given factor. This factor should be between 0.0 and 1.0.
    /// Where 0.0 will leave the color unchanged, and 1.0 will completely fade out the color.
    fn fade_out(&mut self, factor: f32);

    /// Multiplies the alpha value of the color by a given factor and returns a new color.
    /// If the color was previously opaque, then this is equivalent to
    /// [`with_alpha`](palette::WithAlpha::with_alpha).
    ///
    /// Useful for transforming colors with dynamic opacity,
    /// like a color from an external source.
    ///
    /// Example:
    /// ```
    /// use gpui::ColorExt;
    /// let color = gpui::red();
    /// let faded_color = color.opacity(0.5);
    /// assert_eq!(faded_color.alpha, 0.5);
    /// ```
    ///
    /// This will return a red color with half the opacity.
    ///
    /// Example:
    /// ```
    /// use gpui::{hsla, ColorExt};
    /// let color = hsla(0.7, 1.0, 0.5, 0.7); // A saturated blue
    /// let faded_color = color.opacity(0.16);
    /// assert!((faded_color.alpha - 0.112).abs() < 1e-6);
    /// ```
    ///
    /// This will return a blue color with around ~10% opacity,
    /// suitable for an element's hover or selected state.
    ///
    fn opacity(&self, factor: f32) -> Self
    where
        Self: Sized;
}
impl ColorExt for Rgba {
    fn blend(&self, other: &Self) -> Self {
        use palette::blend::{BlendWith, Equations, Parameter};
        let blend_mode =
            Equations::from_parameters(Parameter::OneMinusSourceAlpha, Parameter::SourceAlpha);
        self.blend_with(*other, blend_mode)
    }

    fn fade_out(&mut self, factor: f32) {
        self.alpha *= 1.0 - factor.clamp(0., 1.);
    }

    fn opacity(&self, factor: f32) -> Self {
        let mut color = *self;
        color.alpha *= factor.clamp(0., 1.);
        color
    }
}
impl ColorExt for Hsla {
    fn blend(&self, other: &Self) -> Self {
        let this: Rgba = (*self).into_color();
        let other: Rgba = (*other).into_color();
        this.blend(&other).into_color()
    }

    fn fade_out(&mut self, factor: f32) {
        self.alpha *= 1.0 - factor.clamp(0., 1.);
    }

    fn opacity(&self, factor: f32) -> Self {
        let mut color = *self;
        color.alpha *= factor.clamp(0., 1.);
        color
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub(crate) enum BackgroundTag {
    Solid = 0,
    LinearGradient = 1,
    PatternSlash = 2,
    Checkerboard = 3,
}

/// A color space for color interpolation.
///
/// References:
/// - <https://developer.mozilla.org/en-US/docs/Web/CSS/color-interpolation-method>
/// - <https://www.w3.org/TR/css-color-4/#typedef-color-space>
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub enum ColorSpace {
    #[default]
    /// The sRGB color space.
    Srgb = 0,
    /// The Oklab color space.
    Oklab = 1,
}

impl Display for ColorSpace {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ColorSpace::Srgb => write!(f, "sRGB"),
            ColorSpace::Oklab => write!(f, "Oklab"),
        }
    }
}

/// A background color, which can be either a solid color or a linear gradient.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub struct Background {
    pub(crate) tag: BackgroundTag,
    pub(crate) color_space: ColorSpace,
    pub(crate) solid: crate::SceneHsla,
    pub(crate) gradient_angle_or_pattern_height: f32,
    pub(crate) colors: [LinearColorStop; 2],
    /// Padding for alignment for repr(C) layout.
    pad: u32,
}

impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.tag {
            BackgroundTag::Solid => write!(f, "Solid({:?})", self.solid),
            BackgroundTag::LinearGradient => write!(
                f,
                "LinearGradient({}, {:?}, {:?})",
                self.gradient_angle_or_pattern_height, self.colors[0], self.colors[1]
            ),
            BackgroundTag::PatternSlash => write!(
                f,
                "PatternSlash({:?}, {})",
                self.solid, self.gradient_angle_or_pattern_height
            ),
            BackgroundTag::Checkerboard => write!(
                f,
                "Checkerboard({:?}, {})",
                self.solid, self.gradient_angle_or_pattern_height
            ),
        }
    }
}

impl Eq for Background {}
impl Default for Background {
    fn default() -> Self {
        Self {
            tag: BackgroundTag::Solid,
            solid: Hsla::default().into(),
            color_space: ColorSpace::default(),
            gradient_angle_or_pattern_height: 0.0,
            colors: [LinearColorStop::default(), LinearColorStop::default()],
            pad: 0,
        }
    }
}

/// Creates a hash pattern background
pub fn pattern_slash(color: impl IntoColor<Hsla>, width: f32, interval: f32) -> Background {
    let width_scaled = (width * 255.0) as u32;
    let interval_scaled = (interval * 255.0) as u32;
    let height = ((width_scaled * 0xFFFF) + interval_scaled) as f32;

    Background {
        tag: BackgroundTag::PatternSlash,
        solid: color.into_color().into(),
        gradient_angle_or_pattern_height: height,
        ..Default::default()
    }
}

/// Creates a checkerboard pattern background
pub fn checkerboard(color: impl IntoColor<Hsla>, size: f32) -> Background {
    Background {
        tag: BackgroundTag::Checkerboard,
        solid: color.into_color().into(),
        gradient_angle_or_pattern_height: size,
        ..Default::default()
    }
}

/// Creates a solid background color.
pub fn solid_background(color: impl IntoColor<Hsla>) -> Background {
    Background {
        solid: color.into_color().into(),
        ..Default::default()
    }
}

/// Creates a LinearGradient background color.
///
/// The gradient line's angle of direction. A value of `0.` is equivalent to top; increasing values rotate clockwise from there.
///
/// The `angle` is in degrees value in the range 0.0 to 360.0.
///
/// <https://developer.mozilla.org/en-US/docs/Web/CSS/gradient/linear-gradient>
pub fn linear_gradient(
    angle: f32,
    from: impl Into<LinearColorStop>,
    to: impl Into<LinearColorStop>,
) -> Background {
    Background {
        tag: BackgroundTag::LinearGradient,
        gradient_angle_or_pattern_height: angle,
        colors: [from.into(), to.into()],
        ..Default::default()
    }
}

/// A color stop in a linear gradient.
///
/// <https://developer.mozilla.org/en-US/docs/Web/CSS/gradient/linear-gradient#linear-color-stop>
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub struct LinearColorStop {
    /// The color of the color stop.
    pub color: crate::SceneHsla,
    /// The percentage of the gradient, in the range 0.0 to 1.0.
    pub percentage: f32,
}

/// Creates a new linear color stop.
///
/// The percentage of the gradient, in the range 0.0 to 1.0.
pub fn linear_color_stop(color: impl IntoColor<Hsla>, percentage: f32) -> LinearColorStop {
    LinearColorStop {
        color: color.into_color().into(),
        percentage,
    }
}

impl LinearColorStop {
    /// Returns a new color stop with the same color, but with a modified alpha value.
    pub fn opacity(&self, factor: f32) -> Self {
        let color: Hsla = self.color.into();
        Self {
            percentage: self.percentage,
            color: color.opacity(factor).into(),
        }
    }
}

/// What a [`Background`] paints, decoded from its packed representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundKind {
    /// A flat color.
    Solid(Hsla),
    /// A linear gradient between two color stops.
    LinearGradient {
        /// The gradient line's angle in degrees, `0.0` pointing up, increasing clockwise.
        angle: f32,
        /// The two ends of the gradient.
        stops: [LinearColorStop; 2],
    },
    /// A diagonal stripe pattern.
    PatternSlash {
        /// The stripe color.
        color: Hsla,
        /// The stripe width, in logical pixels.
        width: f32,
        /// The gap between stripes, in logical pixels.
        interval: f32,
    },
    /// Alternating squares of one color and full transparency.
    Checkerboard {
        /// The color of one set of squares. The other set is fully transparent.
        color: Hsla,
        /// The width and height of each square, in logical pixels.
        size: f32,
    },
}

impl Background {
    /// Returns the stable scalar fields consumed by renderer transfer structures.
    #[doc(hidden)]
    pub fn shader_components(&self) -> (u32, u32, crate::SceneHsla, f32, [LinearColorStop; 2]) {
        let tag = match self.tag {
            BackgroundTag::Solid => 0,
            BackgroundTag::LinearGradient => 1,
            BackgroundTag::PatternSlash => 2,
            BackgroundTag::Checkerboard => 3,
        };
        let color_space = match self.color_space {
            ColorSpace::Srgb => 0,
            ColorSpace::Oklab => 1,
        };
        (
            tag,
            color_space,
            self.solid,
            self.gradient_angle_or_pattern_height,
            self.colors,
        )
    }

    /// Returns the solid color if this is a solid background, None otherwise.
    pub fn as_solid(&self) -> Option<Hsla> {
        if self.tag == BackgroundTag::Solid {
            Some(self.solid.into())
        } else {
            None
        }
    }

    /// Returns the decoded form of this background.
    pub fn kind(&self) -> BackgroundKind {
        match self.tag {
            BackgroundTag::Solid => BackgroundKind::Solid(self.solid.into()),
            BackgroundTag::LinearGradient => BackgroundKind::LinearGradient {
                angle: self.gradient_angle_or_pattern_height,
                stops: self.colors,
            },
            BackgroundTag::PatternSlash => {
                // `pattern_slash` packs both values into one f32 as `(width * 255) * 0xFFFF + (interval * 255)`.
                // floor + rem_euclid to invert it since that's the pairing that stays correct for negative inputs.
                // truncation and `%` give the wrong entry.
                let packed = self.gradient_angle_or_pattern_height;
                BackgroundKind::PatternSlash {
                    color: self.solid.into(),
                    width: (packed / 0xFFFF as f32).floor() / 255.0,
                    interval: (packed.rem_euclid(0xFFFF as f32)) / 255.0,
                }
            }
            BackgroundTag::Checkerboard => BackgroundKind::Checkerboard {
                color: self.solid.into(),
                size: self.gradient_angle_or_pattern_height,
            },
        }
    }

    /// Use specified color space for color interpolation.
    ///
    /// <https://developer.mozilla.org/en-US/docs/Web/CSS/color-interpolation-method>
    pub fn color_space(mut self, color_space: ColorSpace) -> Self {
        self.color_space = color_space;
        self
    }

    /// The color space used to interpolate this background, set by [`Background::color_space`].
    pub fn interpolation_space(&self) -> ColorSpace {
        self.color_space
    }

    /// Returns a new background color with the same hue, saturation, and lightness, but with a modified alpha value.
    pub fn opacity(&self, factor: f32) -> Self {
        let mut background = *self;
        let solid: Hsla = background.solid.into();
        background.solid = solid.opacity(factor).into();
        background.colors = [
            self.colors[0].opacity(factor),
            self.colors[1].opacity(factor),
        ];
        background
    }

    /// Returns whether the background color is transparent.
    pub fn is_transparent(&self) -> bool {
        match self.tag {
            BackgroundTag::Solid => self.solid.a == 0.,
            BackgroundTag::LinearGradient => self.colors.iter().all(|c| c.color.a == 0.),
            BackgroundTag::PatternSlash => self.solid.a == 0.,
            BackgroundTag::Checkerboard => self.solid.a == 0.,
        }
    }
}

impl<T: IntoColor<Hsla>> From<T> for Background {
    fn from(value: T) -> Self {
        Self {
            tag: BackgroundTag::Solid,
            solid: value.into_color().into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_solid() {
        let color: Hsla = rgba(0xff0099ff).into_color();
        let mut background = Background::from(color);
        assert_eq!(background.tag, BackgroundTag::Solid);
        assert_eq!(background.solid, color.into());

        assert_eq!(background.opacity(0.5).solid, color.opacity(0.5).into());
        assert!(!background.is_transparent());
        background.solid = hsla(0.0, 0.0, 0.0, 0.0).into();
        assert!(background.is_transparent());
    }

    #[test]
    fn test_background_linear_gradient() {
        let from = linear_color_stop(rgba(0xff0099ff), 0.0);
        let to = linear_color_stop(rgba(0x00ff99ff), 1.0);
        let background = linear_gradient(90.0, from, to);
        assert_eq!(background.tag, BackgroundTag::LinearGradient);
        assert_eq!(background.colors[0], from);
        assert_eq!(background.colors[1], to);

        assert_eq!(background.opacity(0.5).colors[0], from.opacity(0.5));
        assert_eq!(background.opacity(0.5).colors[1], to.opacity(0.5));
        assert!(!background.is_transparent());
        assert!(background.opacity(0.0).is_transparent());
    }

    #[test]
    fn test_background_kind() {
        let color: Hsla = rgba(0xff0099ff).into_color();
        assert_eq!(Background::from(color).kind(), BackgroundKind::Solid(color));

        let from = linear_color_stop(rgba(0xff0099ff), 0.0);
        let to = linear_color_stop(rgba(0x00ff99ff), 1.0);
        assert_eq!(
            linear_gradient(90.0, from, to).kind(),
            BackgroundKind::LinearGradient {
                angle: 90.0,
                stops: [from, to],
            }
        );

        assert_eq!(
            checkerboard(color, 12.0).kind(),
            BackgroundKind::Checkerboard { color, size: 12.0 }
        );
    }

    #[test]
    fn test_background_kind_unpacks_pattern_slash() {
        let color: Hsla = rgba(0xff0099ff).into_color();
        // Both values survive to the 1/255 the constructor quantizes them to.
        for (width, interval) in [(1.0, 3.0), (0.5, 0.25), (2.0, 10.0)] {
            let BackgroundKind::PatternSlash {
                width: got_width,
                interval: got_interval,
                ..
            } = pattern_slash(color, width, interval).kind()
            else {
                panic!("pattern_slash did not produce a PatternSlash");
            };
            assert!((got_width - width).abs() <= 1.0 / 255.0);
            assert!((got_interval - interval).abs() <= 1.0 / 255.0);
        }
    }

    #[test]
    fn test_rgba_alpha() {
        use palette::WithAlpha;
        let color = Rgba::<palette::Srgb>::new(0.2, 0.6, 1.0, 0.8);
        assert_eq!(color.with_alpha(0.25).alpha, 0.25);
        // NOTE: diverging from upstream, where Rgba::alpha clamps. palette does not clamp alpha
        assert_eq!(color.with_alpha(1.5).alpha, 1.5);
    }

    #[test]
    fn test_rgba_opacity() {
        use super::ColorExt;
        let color = Rgba::new(0.2, 0.6, 1.0, 0.8);
        assert!((color.opacity(0.5).alpha - 0.4).abs() < 1e-6);
        assert_eq!(color.opacity(2.0).alpha, 0.8);
    }
}
