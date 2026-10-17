//! Which inspector controls are on screen.
//!
//! This only decides `kOfxParamPropSecret`. It does not read or write
//! stabilization values. DaVinci Resolve treats a parameter that was secret in
//! the descriptor as permanently hidden, so every name here is described
//! visible and hidden later on the instance.

/// View menu. Index 0 is the OFX choice default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelView {
    Full = 0,
    Minimal = 1,
    Custom = 2,
}

impl PanelView {
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Minimal,
            2 => Self::Custom,
            _ => Self::Full,
        }
    }
}

/// Custom-mode checkboxes. Full and Minimal ignore these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelGroups {
    pub project: bool,
    pub stabilization: bool,
    pub adjustments: bool,
    pub keyframes: bool,
    pub output: bool,
}

impl PanelGroups {
    pub fn all() -> Self {
        Self { project: true, stabilization: true, adjustments: true, keyframes: true, output: true }
    }
}

/// Parameters whose secret flag follows the view. Hidden plugin data
/// (instance id, embedded project, the zoom-limit override flag) is not here:
/// those stay secret from the descriptor.
pub const VISIBILITY_PARAMS: &[&str] = &[
    "View",
    "VisibleGroups",
    "ShowProject",
    "ShowStabilization",
    "ShowAdjustments",
    "ShowKeyframes",
    "ShowOutput",
    "ProjectGroup",
    "LoadCurrent",
    "gyrodata",
    "Browse",
    "LoadLens",
    "OpenGyroflow",
    "ReloadProject",
    "OpenRecentProject",
    "Status",
    "AdjustGroup",
    "FOV",
    "Smoothness",
    "LensCorrectionStrength",
    "HorizonLockAmount",
    "HorizonLockRoll",
    "PositionX",
    "PositionY",
    "ZoomLimit",
    "RotationOverride",
    "InputRotation",
    "Rotation",
    "VideoSpeed",
    "DisableStretch",
    "KeyframesGroup",
    "UseGyroflowsKeyframes",
    "RecalculateKeyframes",
    "ToggleOverview",
    "DontDrawOutside",
    "IncludeProjectData",
];

pub fn show_custom_toggles(view: PanelView) -> bool {
    view == PanelView::Custom
}

pub fn groups_for(view: PanelView, custom: PanelGroups) -> PanelGroups {
    match view {
        PanelView::Full => PanelGroups::all(),
        PanelView::Minimal => PanelGroups {
            project: true,
            stabilization: true,
            adjustments: false,
            keyframes: false,
            output: false,
        },
        PanelView::Custom => custom,
    }
}

/// `true` means set `kOfxParamPropSecret` so the host hides the parameter.
///
/// Unknown names stay visible. The always-secret plugin data must not be
/// passed through here, or a Full view would reveal it.
pub fn parameter_is_secret(name: &str, view: PanelView, custom: PanelGroups) -> bool {
    let groups = groups_for(view, custom);
    match name {
        "View" => false,
        "VisibleGroups" | "ShowProject" | "ShowStabilization" | "ShowAdjustments" | "ShowKeyframes" | "ShowOutput" => {
            !show_custom_toggles(view)
        }
        "ProjectGroup" | "LoadCurrent" | "gyrodata" | "Browse" | "LoadLens" | "OpenGyroflow"
        | "ReloadProject" | "OpenRecentProject" | "Status" => !groups.project,
        "Smoothness" | "HorizonLockAmount" | "ZoomLimit" | "RotationOverride" => !groups.stabilization,
        "FOV" | "LensCorrectionStrength" | "HorizonLockRoll" | "PositionX" | "PositionY"
        | "InputRotation" | "Rotation" | "VideoSpeed" | "DisableStretch" => !groups.adjustments,
        // Stabilization controls live in this group, so the header stays while
        // any of those controls are on screen.
        "AdjustGroup" => !(groups.stabilization || groups.adjustments),
        "KeyframesGroup" | "UseGyroflowsKeyframes" | "RecalculateKeyframes" => !groups.keyframes,
        "ToggleOverview" | "DontDrawOutside" | "IncludeProjectData" => !groups.output,
        _ => false,
    }
}

/// Zoom limit to write into the stabilizer.
///
/// `None` leaves the value loaded from the project file. The slider starts at
/// Gyroflow's default (130) and must not overwrite a project that uses another
/// limit until the user moves it.
pub fn applied_zoom_limit(user_override: bool, slider: f64) -> Option<f64> {
    if user_override && slider.is_finite() { Some(slider) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn off() -> PanelGroups {
        PanelGroups { project: false, stabilization: false, adjustments: false, keyframes: false, output: false }
    }

    #[test]
    fn full_shows_every_control_and_hides_the_custom_section() {
        let custom = off();
        for name in ["FOV", "Smoothness", "ZoomLimit", "HorizonLockAmount", "RotationOverride", "VideoSpeed", "UseGyroflowsKeyframes", "ToggleOverview", "gyrodata"] {
            assert!(!parameter_is_secret(name, PanelView::Full, custom), "{name} hidden in Full");
        }
        assert!(parameter_is_secret("ShowProject", PanelView::Full, custom));
        assert!(parameter_is_secret("VisibleGroups", PanelView::Full, custom));
        assert!(!parameter_is_secret("View", PanelView::Full, custom));
    }

    #[test]
    fn minimal_keeps_the_daily_controls() {
        let custom = PanelGroups::all();
        for name in ["gyrodata", "Browse", "LoadCurrent", "OpenGyroflow", "ReloadProject", "Status", "Smoothness", "HorizonLockAmount", "ZoomLimit", "RotationOverride", "View"] {
            assert!(!parameter_is_secret(name, PanelView::Minimal, custom), "{name} should stay in Minimal");
        }
        for name in ["FOV", "LensCorrectionStrength", "HorizonLockRoll", "PositionX", "InputRotation", "Rotation", "VideoSpeed", "DisableStretch", "UseGyroflowsKeyframes", "ToggleOverview", "DontDrawOutside", "IncludeProjectData", "ShowStabilization"] {
            assert!(parameter_is_secret(name, PanelView::Minimal, custom), "{name} should leave Minimal");
        }
        // The daily controls are children of Adjust parameters.
        assert!(!parameter_is_secret("AdjustGroup", PanelView::Minimal, custom));
    }

    #[test]
    fn custom_uses_the_checkboxes_and_shows_them() {
        let mut custom = off();
        custom.stabilization = true;
        assert!(!parameter_is_secret("ShowProject", PanelView::Custom, custom));
        assert!(!parameter_is_secret("ZoomLimit", PanelView::Custom, custom));
        assert!(parameter_is_secret("gyrodata", PanelView::Custom, custom));
        assert!(parameter_is_secret("FOV", PanelView::Custom, custom));
        assert!(!parameter_is_secret("AdjustGroup", PanelView::Custom, custom));
        custom.stabilization = false;
        assert!(parameter_is_secret("AdjustGroup", PanelView::Custom, custom));
        assert!(parameter_is_secret("ZoomLimit", PanelView::Custom, custom));
    }

    #[test]
    fn unknown_and_unlisted_names_stay_visible() {
        assert!(!parameter_is_secret("InstanceId", PanelView::Minimal, off()));
        assert!(!parameter_is_secret("ZoomLimitOverride", PanelView::Full, off()));
    }

    #[test]
    fn every_visibility_param_can_be_hidden_except_the_view_menu() {
        let custom = off();
        for name in VISIBILITY_PARAMS {
            if *name == "View" {
                assert!(!parameter_is_secret(name, PanelView::Minimal, custom));
                continue;
            }
            let hidden = [PanelView::Full, PanelView::Minimal, PanelView::Custom]
                .into_iter()
                .any(|view| parameter_is_secret(name, view, custom));
            assert!(hidden, "{name} is never hidden");
        }
    }

    #[test]
    fn zoom_limit_stays_in_the_project_until_the_user_moves_it() {
        assert_eq!(applied_zoom_limit(false, 130.0), None);
        assert_eq!(applied_zoom_limit(false, 180.0), None);
        assert_eq!(applied_zoom_limit(true, 160.0), Some(160.0));
        assert_eq!(applied_zoom_limit(true, f64::NAN), None);
        assert_eq!(PanelView::from_index(99), PanelView::Full);
    }
}
