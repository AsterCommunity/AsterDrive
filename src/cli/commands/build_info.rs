//! Build identity presentation for the version command.

pub(in crate::cli) fn render_build_info(json: bool) -> crate::errors::Result<String> {
    #[derive(serde::Serialize)]
    struct BuildInfo {
        product: &'static str,
        version: &'static str,
        revision: &'static str,
        dirty: Option<bool>,
        target: &'static str,
        profile: &'static str,
        features: Vec<&'static str>,
        build_time: &'static str,
    }
    let info = BuildInfo {
        product: "AsterDrive",
        version: env!("CARGO_PKG_VERSION"),
        revision: env!("ASTER_BUILD_REVISION"),
        dirty: match env!("ASTER_BUILD_DIRTY") {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        target: env!("ASTER_BUILD_TARGET"),
        profile: env!("ASTER_BUILD_PROFILE"),
        features: env!("ASTER_BUILD_FEATURES")
            .split(',')
            .filter(|feature| !feature.is_empty())
            .collect(),
        build_time: env!("ASTER_BUILD_TIME"),
    };
    if json {
        return serde_json::to_string_pretty(&info).map_err(|_| {
            crate::errors::AsterError::internal_error("cannot serialize build information")
        });
    }
    let revision = match info.dirty {
        Some(true) => format!("{} (dirty)", info.revision),
        Some(false) => info.revision.to_owned(),
        None => format!("{} (working tree unknown)", info.revision),
    };
    Ok([
        format!("{} {}", info.product, info.version),
        format!("Revision:  {revision}"),
        format!("Target:    {}", info.target),
        format!("Profile:   {}", info.profile),
        format!(
            "Features:  {}",
            if info.features.is_empty() {
                "none".into()
            } else {
                info.features.join(", ")
            }
        ),
        format!("Built:     {}", info.build_time),
    ]
    .join("\n"))
}
