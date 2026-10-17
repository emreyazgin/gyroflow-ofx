//! How a host turns a clip before Gyroflow sees it, and the rectangles that keep
//! that turn from stretching the frame.
//!
//! A sideways camera stores a landscape frame plus a 90° or 270° rotation flag.
//! DaVinci Resolve applies that flag itself (Clip Rotation) and hands the plugin
//! upright pixels. Gyroflow still stabilizes the stored, unturned frame. Cropping
//! those upright pixels with the stored aspect, or drawing the portrait result
//! into a landscape buffer, squeezes the image. The turn computed here is the
//! host's pre-rotation; the rects sample and draw at the turned aspect.

/// Rotation Override menu. Index 0 is the OFX choice default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotationOverride {
    Auto = 0,
    VerticalVideo = 1,
}

impl RotationOverride {
    pub fn from_index(index: i32) -> Self {
        if index == 1 { Self::VerticalVideo } else { Self::Auto }
    }
}

/// Which host buffer this instance is drawing into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPage {
    /// Resolve Edit or Color. The output buffer is the timeline, and Resolve has
    /// already applied the clip's metadata rotation.
    ResolveTimeline,
    /// Resolve Fusion. The clip is handed over at its own size. A buffer with the
    /// stored frame's shape means Fusion did not turn it.
    ResolveFusion,
    /// Another OFX host. Landscape clips keep the previous behavior unless the
    /// buffer is visibly the turned frame.
    Other,
}

impl HostPage {
    pub fn from_resolve_page(page: &str) -> Self {
        match page {
            "Edit" | "Color" => Self::ResolveTimeline,
            "Fusion" => Self::ResolveFusion,
            _ => Self::Other,
        }
    }

    /// The host buffer size is the timeline and the plugin cannot change it.
    pub fn fixes_timeline_output(self) -> bool {
        matches!(self, Self::ResolveTimeline)
    }
}

/// Where the turned clip sits in the host buffers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameGeometry {
    /// Degrees passed to the stabilization kernel (this plugin's framebuffer is inverted).
    pub kernel_rotation: f64,
    /// x, y, width, height of the clip inside the input buffer.
    pub src_rect: (usize, usize, usize, usize),
    /// Where the stabilized image is drawn. `None` fills the output buffer.
    pub out_rect: Option<(usize, usize, usize, usize)>,
}

/// Aspect turn and kernel turn. They match when the host's pre-rotation is
/// being compensated. On other hosts they stay split, so a non-zero Input
/// rotation keeps rotating samples without changing the stored aspect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostTurn {
    pub aspect: f64,
    pub kernel: f64,
}

pub fn normalize_degrees(rotation: f64) -> f64 {
    if !rotation.is_finite() {
        return 0.0;
    }
    let r = rotation.rem_euclid(360.0);
    if (r - 360.0).abs() < 1e-6 { 0.0 } else { r }
}

/// A turn that swaps width and height.
pub fn is_quarter_turn(rotation: f64) -> bool {
    (normalize_degrees(rotation) % 180.0 - 90.0).abs() < 1.0
}

/// `size` after a turn of `rotation` degrees.
pub fn rotated_size(size: (usize, usize), rotation: f64) -> (usize, usize) {
    if is_quarter_turn(rotation) { (size.1, size.0) } else { size }
}

/// Input rotation Gyroflow uses for a container rotation flag.
///
/// The host shows the file upright, so the plugin has to sample backwards from
/// that upright image to the stored frame. 90° of metadata becomes 270°, and
/// 270° becomes 90°.
pub fn host_turn_from_metadata(metadata_rotation: f64) -> f64 {
    let r = normalize_degrees(metadata_rotation).round() as i32;
    ((360 - r.rem_euclid(360)) % 360) as f64
}

fn angle_distance(a: f64, b: f64) -> f64 {
    let d = (normalize_degrees(a) - normalize_degrees(b)).abs();
    d.min(360.0 - d)
}

fn aspect(size: (usize, usize)) -> f64 {
    if size.1 == 0 { return 0.0; }
    size.0 as f64 / size.1 as f64
}

fn aspects_match(a: (usize, usize), b: (usize, usize)) -> bool {
    if a.0 == 0 || a.1 == 0 || b.0 == 0 || b.1 == 0 {
        return false;
    }
    let (aa, bb) = (aspect(a), aspect(b));
    if aa == 0.0 || bb == 0.0 { return false; }
    (aa / bb - 1.0).abs() < 0.02
}

/// The buffer has the stored frame's aspect, and not the quarter-turned one.
fn buffer_is_unturned(video_size: (usize, usize), src_size: (usize, usize)) -> bool {
    aspects_match(src_size, video_size) && !aspects_match(src_size, rotated_size(video_size, 90.0))
}

/// The buffer has the quarter-turned aspect, and not the stored one.
fn buffer_is_turned(video_size: (usize, usize), src_size: (usize, usize)) -> bool {
    let turned = rotated_size(video_size, 90.0);
    aspects_match(src_size, turned) && !aspects_match(src_size, video_size)
}

/// Portrait handling for Rotation Override → Vertical Video.
///
/// A 90° or 270° container flag selects the direction (mapped the same way Auto
/// maps it). With no flag, a frame that is already taller than it is wide stays
/// unturned; a landscape frame is turned 90° so the result is portrait.
pub fn vertical_host_turn(metadata_rotation: f64, video_size: (usize, usize)) -> f64 {
    let from_meta = host_turn_from_metadata(metadata_rotation);
    if is_quarter_turn(from_meta) {
        return from_meta;
    }
    if video_size.1 > video_size.0 && video_size.0 > 0 {
        0.0
    } else {
        90.0
    }
}

/// Host pre-rotation for Rotation Override → Auto.
pub fn auto_host_turn(
    metadata_rotation: f64,
    input_rotation_slider: f64,
    video_size: (usize, usize),
    src_size: (usize, usize),
    page: HostPage,
) -> f64 {
    let from_meta = host_turn_from_metadata(metadata_rotation);
    let slider = normalize_degrees(input_rotation_slider);
    // The Input rotation slider is the older manual control. A value that is
    // neither 0 nor the metadata turn is an explicit choice and wins.
    let slider_is_custom = angle_distance(slider, 0.0) > 0.5 && angle_distance(slider, from_meta) > 0.5;

    match page {
        HostPage::ResolveTimeline => {
            if slider_is_custom { slider } else { from_meta }
        }
        HostPage::ResolveFusion => {
            let turn = if slider_is_custom { slider } else { from_meta };
            if is_quarter_turn(turn) && buffer_is_unturned(video_size, src_size) {
                0.0
            } else {
                turn
            }
        }
        HostPage::Other => {
            if angle_distance(slider, 0.0) > 0.5 {
                slider
            } else if is_quarter_turn(from_meta) && buffer_is_turned(video_size, src_size) {
                from_meta
            } else {
                0.0
            }
        }
    }
}

pub fn resolve_host_turn(
    mode: RotationOverride,
    metadata_rotation: f64,
    input_rotation_slider: f64,
    video_size: (usize, usize),
    src_size: (usize, usize),
    page: HostPage,
) -> HostTurn {
    if mode == RotationOverride::VerticalVideo {
        let turn = vertical_host_turn(metadata_rotation, video_size);
        return HostTurn { aspect: turn, kernel: turn };
    }
    match page {
        HostPage::Other => {
            let from_meta = host_turn_from_metadata(metadata_rotation);
            // Only a buffer that is already portrait (or otherwise quarter-turned)
            // is compensated. Otherwise Input rotation keeps its old job: it
            // rotates samples, and the source rect stays the stored aspect.
            if is_quarter_turn(from_meta) && buffer_is_turned(video_size, src_size) && angle_distance(input_rotation_slider, 0.0) <= 0.5 {
                HostTurn { aspect: from_meta, kernel: from_meta }
            } else if is_quarter_turn(from_meta) && buffer_is_turned(video_size, src_size) {
                let turn = normalize_degrees(input_rotation_slider);
                HostTurn { aspect: turn, kernel: turn }
            } else {
                HostTurn { aspect: 0.0, kernel: normalize_degrees(input_rotation_slider) }
            }
        }
        HostPage::ResolveTimeline | HostPage::ResolveFusion => {
            let turn = auto_host_turn(metadata_rotation, input_rotation_slider, video_size, src_size, page);
            HostTurn { aspect: turn, kernel: turn }
        }
    }
}

/// Centered rect of `org_ratio` inside `width`×`height`.
///
/// The same test the plugin has always used: aspects within 0.1 share the whole
/// buffer, so a landscape clip in a landscape timeline is not cropped.
pub fn get_center_rect(width: usize, height: usize, org_ratio: f64) -> (usize, usize, usize, usize) {
    if width == 0 || height == 0 || !org_ratio.is_finite() || org_ratio <= 0.0 {
        return (0, 0, width, height);
    }
    let new_ratio = width as f64 / height as f64;
    if (new_ratio - org_ratio).abs() > 0.1 {
        let rect = if new_ratio > org_ratio {
            ((height as f64 * org_ratio).round() as usize, height)
        } else {
            (width, (width as f64 / org_ratio).round() as usize)
        };
        let rect = (rect.0.min(width), rect.1.min(height));
        (
            (width - rect.0) / 2,
            (height - rect.1) / 2,
            rect.0,
            rect.1,
        )
    } else {
        (0, 0, width, height)
    }
}

/// Size the stabilizer should render at.
///
/// Landscape clips (no quarter turn, or a landscape timeline that already matches)
/// keep the host buffer, which is what the plugin did before. A portrait clip on
/// Resolve's timeline is stabilized at the portrait size so it can be pillarboxed
/// instead of stretched. Fusion and other hosts that can change their output get
/// the turned clip size directly.
pub fn stabilization_output_size(
    video_size: (usize, usize),
    host_turn: f64,
    out_size: (usize, usize),
    page: HostPage,
) -> (usize, usize) {
    if out_size.0 == 0 || out_size.1 == 0 {
        let clip = rotated_size(video_size, host_turn);
        return if clip.0 > 0 && clip.1 > 0 { clip } else { out_size };
    }
    // Resolve draws clip and node thumbnails through the plugin at its own small
    // sizes. Those must not rebuild the stabilization at the full clip size.
    if out_size.0.max(out_size.1) < 320 {
        return out_size;
    }
    let clip = rotated_size(video_size, host_turn);
    if clip.0 == 0 || clip.1 == 0 {
        return out_size;
    }
    let portrait_clip = clip.1 > clip.0;
    let horizontal_buffer = out_size.0 > out_size.1;
    if page.fixes_timeline_output() && portrait_clip && horizontal_buffer {
        return clip;
    }
    if !page.fixes_timeline_output() && is_quarter_turn(host_turn) {
        return clip;
    }
    out_size
}

pub fn frame_geometry(
    video_size: (usize, usize),
    host_turn: HostTurn,
    src_size: (usize, usize),
    out_size: (usize, usize),
    page: HostPage,
    dont_draw_outside: bool,
) -> FrameGeometry {
    let clip = rotated_size(video_size, host_turn.aspect);
    let clip_ratio = if clip.0 > 0 && clip.1 > 0 {
        clip.0 as f64 / clip.1 as f64
    } else if video_size.1 > 0 {
        video_size.0 as f64 / video_size.1 as f64
    } else {
        1.0
    };
    let src_rect = if video_size.0 == 0 || video_size.1 == 0 {
        (0, 0, src_size.0, src_size.1)
    } else {
        get_center_rect(src_size.0, src_size.1, clip_ratio)
    };

    let portrait_in_horizontal = page.fixes_timeline_output()
        && clip.1 > clip.0
        && out_size.0 > out_size.1
        && out_size.0.max(out_size.1) >= 320;

    let out_rect = if dont_draw_outside {
        let output_ratio = if out_size.1 > 0 {
            out_size.0 as f64 / out_size.1 as f64
        } else {
            clip_ratio
        };
        let mut rect = get_center_rect(src_rect.2, src_rect.3, output_ratio);
        rect.0 += src_rect.0;
        rect.1 += src_rect.1;
        Some(rect)
    } else if portrait_in_horizontal {
        Some(get_center_rect(out_size.0, out_size.1, clip_ratio))
    } else {
        None
    };

    FrameGeometry {
        kernel_rotation: host_turn.kernel,
        src_rect,
        out_rect,
    }
}

/// Output region for hosts that can adopt the clip's own size.
///
/// Resolve's Edit and Color pages keep the timeline region; the portrait image
/// is fitted inside that buffer instead. `None` means "use the plugin's
/// existing region", which leaves landscape clips unchanged.
pub fn custom_output_region(
    video_size: (usize, usize),
    host_turn: f64,
    page: HostPage,
) -> Option<(usize, usize)> {
    if page.fixes_timeline_output() || !is_quarter_turn(host_turn) {
        return None;
    }
    let clip = rotated_size(video_size, host_turn);
    if clip.0 == 0 || clip.1 == 0 { None } else { Some(clip) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANDSCAPE: (usize, usize) = (1920, 1080);
    const PORTRAIT: (usize, usize) = (1080, 1920);

    fn rect_aspect(rect: (usize, usize, usize, usize)) -> f64 {
        rect.2 as f64 / rect.3.max(1) as f64
    }

    #[test]
    fn metadata_direction_swaps_90_and_270() {
        assert!((host_turn_from_metadata(0.0) - 0.0).abs() < 1e-6);
        assert!((host_turn_from_metadata(90.0) - 270.0).abs() < 1e-6);
        assert!((host_turn_from_metadata(270.0) - 90.0).abs() < 1e-6);
        assert!((host_turn_from_metadata(180.0) - 180.0).abs() < 1e-6);
        assert!((host_turn_from_metadata(-90.0) - 90.0).abs() < 1e-6);
    }

    #[test]
    fn auto_leaves_landscape_resolve_clip_untouched() {
        let turn = resolve_host_turn(RotationOverride::Auto, 0.0, 0.0, LANDSCAPE, LANDSCAPE, HostPage::ResolveTimeline);
        assert!((turn.aspect - 0.0).abs() < 1e-6 && (turn.kernel - 0.0).abs() < 1e-6);
        let geometry = frame_geometry(LANDSCAPE, turn, LANDSCAPE, LANDSCAPE, HostPage::ResolveTimeline, false);
        assert_eq!(geometry.src_rect, (0, 0, 1920, 1080));
        assert_eq!(geometry.out_rect, None);
        assert!((geometry.kernel_rotation - 0.0).abs() < 1e-6);
        assert_eq!(
            stabilization_output_size(LANDSCAPE, turn.aspect, LANDSCAPE, HostPage::ResolveTimeline),
            LANDSCAPE
        );
    }

    #[test]
    fn auto_resolve_timeline_uses_metadata_and_keeps_portrait_aspect() {
        // Stored 1920x1080 with a 90° flag. Resolve's vertical timeline is 1080x1920.
        let turn = resolve_host_turn(RotationOverride::Auto, 90.0, 0.0, LANDSCAPE, PORTRAIT, HostPage::ResolveTimeline);
        assert!((turn.aspect - 270.0).abs() < 1e-6, "turn {}", turn.aspect);
        let geometry = frame_geometry(LANDSCAPE, turn, PORTRAIT, PORTRAIT, HostPage::ResolveTimeline, false);
        assert_eq!(geometry.src_rect, (0, 0, 1080, 1920), "portrait buffer is not cropped to landscape");
        assert_eq!(geometry.out_rect, None);
        let ratio = rect_aspect(geometry.src_rect);
        assert!(ratio < 1.0, "source window stays portrait, ratio {ratio}");
        assert_eq!(
            stabilization_output_size(LANDSCAPE, turn.aspect, PORTRAIT, HostPage::ResolveTimeline),
            PORTRAIT
        );
    }

    #[test]
    fn auto_pillarboxes_portrait_clip_on_a_landscape_timeline() {
        let turn = resolve_host_turn(RotationOverride::Auto, 270.0, 0.0, LANDSCAPE, LANDSCAPE, HostPage::ResolveTimeline);
        assert!((turn.aspect - 90.0).abs() < 1e-6, "270° metadata turns the other way, got {}", turn.aspect);
        let geometry = frame_geometry(LANDSCAPE, turn, LANDSCAPE, LANDSCAPE, HostPage::ResolveTimeline, false);
        let out = geometry.out_rect.expect("portrait image is fitted inside the landscape timeline");
        let ratio = rect_aspect(out);
        assert!(ratio < 1.0, "output window is portrait, ratio {ratio}");
        assert!(out.2 < 1920 && out.3 == 1080);
        assert_eq!(
            stabilization_output_size(LANDSCAPE, turn.aspect, LANDSCAPE, HostPage::ResolveTimeline),
            PORTRAIT
        );
    }

    #[test]
    fn vertical_override_picks_metadata_direction() {
        assert!((vertical_host_turn(90.0, LANDSCAPE) - 270.0).abs() < 1e-6);
        assert!((vertical_host_turn(270.0, LANDSCAPE) - 90.0).abs() < 1e-6);
        // No flag and a landscape file: still force a portrait turn.
        assert!((vertical_host_turn(0.0, LANDSCAPE) - 90.0).abs() < 1e-6);
        // Already stored portrait, no flag: do not turn it sideways.
        assert!((vertical_host_turn(0.0, PORTRAIT) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn vertical_override_forces_portrait_even_when_auto_would_not() {
        // Other hosts do not assume a turn when the buffer is still landscape.
        let auto = resolve_host_turn(RotationOverride::Auto, 90.0, 0.0, LANDSCAPE, LANDSCAPE, HostPage::Other);
        assert!((auto.aspect - 0.0).abs() < 1e-6);
        let forced = resolve_host_turn(RotationOverride::VerticalVideo, 90.0, 0.0, LANDSCAPE, LANDSCAPE, HostPage::Other);
        assert!((forced.aspect - 270.0).abs() < 1e-6 && (forced.kernel - 270.0).abs() < 1e-6);
        let geometry = frame_geometry(LANDSCAPE, forced, LANDSCAPE, LANDSCAPE, HostPage::Other, false);
        // Not a Resolve timeline, so the host can adopt the portrait output size
        // instead of stretching into the landscape buffer.
        assert_eq!(geometry.out_rect, None);
        assert_eq!(
            stabilization_output_size(LANDSCAPE, forced.aspect, LANDSCAPE, HostPage::Other),
            PORTRAIT
        );
        assert_eq!(custom_output_region(LANDSCAPE, forced.aspect, HostPage::Other), Some(PORTRAIT));
        assert_eq!(custom_output_region(LANDSCAPE, forced.aspect, HostPage::ResolveTimeline), None);
    }

    #[test]
    fn fusion_does_not_turn_a_buffer_that_is_still_the_stored_frame() {
        let turn = resolve_host_turn(RotationOverride::Auto, 90.0, 0.0, LANDSCAPE, LANDSCAPE, HostPage::ResolveFusion);
        assert!((turn.aspect - 0.0).abs() < 1e-6);
        let turned = resolve_host_turn(RotationOverride::Auto, 90.0, 0.0, LANDSCAPE, PORTRAIT, HostPage::ResolveFusion);
        assert!((turned.aspect - 270.0).abs() < 1e-6);
    }

    #[test]
    fn other_host_compensates_only_when_the_buffer_was_turned() {
        let landscape = resolve_host_turn(RotationOverride::Auto, 0.0, 0.0, LANDSCAPE, LANDSCAPE, HostPage::Other);
        assert!((landscape.aspect - 0.0).abs() < 1e-6 && (landscape.kernel - 0.0).abs() < 1e-6);
        let stored = resolve_host_turn(RotationOverride::Auto, 90.0, 0.0, LANDSCAPE, LANDSCAPE, HostPage::Other);
        assert!((stored.aspect - 0.0).abs() < 1e-6, "an unturned buffer keeps the old path");
        let turned = resolve_host_turn(RotationOverride::Auto, 90.0, 0.0, LANDSCAPE, PORTRAIT, HostPage::Other);
        assert!((turned.aspect - 270.0).abs() < 1e-6 && (turned.kernel - 270.0).abs() < 1e-6);
    }

    #[test]
    fn manual_input_rotation_still_wins_on_other_hosts() {
        let turn = resolve_host_turn(RotationOverride::Auto, 0.0, 90.0, LANDSCAPE, LANDSCAPE, HostPage::Other);
        assert!((turn.kernel - 90.0).abs() < 1e-6);
        assert!((turn.aspect - 0.0).abs() < 1e-6, "other hosts keep the stored aspect");
    }

    #[test]
    fn center_rect_keeps_matching_landscape_aspect() {
        assert_eq!(get_center_rect(1920, 1080, 1920.0 / 1080.0), (0, 0, 1920, 1080));
        assert_eq!(get_center_rect(3840, 2160, 16.0 / 9.0), (0, 0, 3840, 2160));
    }

    #[test]
    fn thumbnails_do_not_adopt_the_full_clip_size() {
        let thumb = (320 - 1, 180);
        assert_eq!(
            stabilization_output_size(LANDSCAPE, 270.0, thumb, HostPage::ResolveTimeline),
            thumb
        );
    }
}
