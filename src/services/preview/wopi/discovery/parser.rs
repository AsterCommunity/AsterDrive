use xmltree::{Element, XMLNode};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::preview::wopi::proof::{WopiProofKeySet, parse_wopi_proof_key_set};

use super::types::{WopiDiscovery, WopiDiscoveryAction};

pub(crate) fn parse_discovery_xml(xml: &str) -> Result<WopiDiscovery> {
    let root = Element::parse(xml.as_bytes())
        .map_aster_err_ctx("invalid WOPI discovery XML", AsterError::validation_error)?;
    let mut actions = Vec::new();
    let mut proof_keys = None;
    collect_discovery_proof_keys(&root, &mut proof_keys)?;
    collect_discovery_actions(&root, None, None, &mut actions);
    if actions.is_empty() {
        return Err(AsterError::validation_error(
            "WOPI discovery did not expose any actions",
        ));
    }

    Ok(WopiDiscovery {
        actions,
        proof_keys,
    })
}

fn collect_discovery_proof_keys(
    element: &Element,
    out: &mut Option<WopiProofKeySet>,
) -> Result<()> {
    if element.name.eq_ignore_ascii_case("proof-key") {
        let current_modulus = element_attribute(element, "modulus")
            .ok_or_else(|| AsterError::validation_error("WOPI proof-key is missing modulus"))?;
        let current_exponent = element_attribute(element, "exponent")
            .ok_or_else(|| AsterError::validation_error("WOPI proof-key is missing exponent"))?;
        let parsed = parse_wopi_proof_key_set(
            current_modulus,
            current_exponent,
            element_attribute(element, "oldmodulus"),
            element_attribute(element, "oldexponent"),
        )?;
        if out.replace(parsed).is_some() {
            return Err(AsterError::validation_error(
                "WOPI discovery contains multiple proof-key elements",
            ));
        }
    }

    for child in &element.children {
        if let XMLNode::Element(child) = child {
            collect_discovery_proof_keys(child, out)?;
        }
    }

    Ok(())
}

fn collect_discovery_actions(
    element: &Element,
    app_name: Option<&str>,
    app_icon_url: Option<&str>,
    out: &mut Vec<WopiDiscoveryAction>,
) {
    let (next_app_name, next_app_icon_url) = if element.name.eq_ignore_ascii_case("app") {
        (
            element_attribute(element, "name").or(app_name),
            element_attribute(element, "favIconUrl").or(app_icon_url),
        )
    } else {
        (app_name, app_icon_url)
    };

    if element.name.eq_ignore_ascii_case("action") {
        let action =
            element_attribute(element, "name").map(|value| value.trim().to_ascii_lowercase());
        let urlsrc = element_attribute(element, "urlsrc").map(|value| value.trim().to_string());
        if let (Some(action), Some(urlsrc)) = (action, urlsrc)
            && !action.is_empty()
            && !urlsrc.is_empty()
        {
            let ext = element_attribute(element, "ext")
                .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
                .filter(|value| !value.is_empty());
            let mime = next_app_name
                .map(str::trim)
                .filter(|value| value.contains('/'))
                .map(|value| value.to_ascii_lowercase());
            out.push(WopiDiscoveryAction {
                action,
                app_icon_url: next_app_icon_url.map(str::trim).map(ToString::to_string),
                app_name: next_app_name.map(str::trim).map(ToString::to_string),
                ext,
                mime,
                urlsrc,
            });
        }
    }

    for child in &element.children {
        if let XMLNode::Element(child) = child {
            collect_discovery_actions(child, next_app_name, next_app_icon_url, out);
        }
    }
}

fn element_attribute<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element.attributes.iter().find_map(|(key, value)| {
        if key.eq_ignore_ascii_case(name) {
            Some(value.as_str())
        } else {
            None
        }
    })
}
