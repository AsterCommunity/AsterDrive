use aster_forge_xml::{
    XmlElementEvent, XmlEvent, XmlSafetyPolicy, XmlWalkError, walk_validated_xml,
};

use crate::errors::{AsterError, Result};
use crate::services::preview::wopi::proof::{WopiProofKeySet, parse_wopi_proof_key_set};

use super::types::{WopiDiscovery, WopiDiscoveryAction};

pub(crate) fn parse_discovery_xml(xml: &str) -> Result<WopiDiscovery> {
    let mut app_contexts = Vec::new();
    let mut actions = Vec::new();
    let mut proof_keys = None;

    walk_validated_xml(xml.as_bytes(), XmlSafetyPolicy::untrusted(), |event| {
        match event {
            XmlEvent::Start(element) => {
                let context =
                    handle_element(&element, app_contexts.last(), &mut actions, &mut proof_keys)?;
                app_contexts.push(context);
            }
            XmlEvent::Empty(element) => {
                handle_element(&element, app_contexts.last(), &mut actions, &mut proof_keys)?;
            }
            XmlEvent::End { .. } => {
                app_contexts.pop();
            }
            _ => {}
        }
        Ok(())
    })
    .map_err(|error| match error {
        XmlWalkError::Xml(_) => AsterError::validation_error("invalid WOPI discovery XML"),
        XmlWalkError::Visitor(error) => error,
    })?;

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

#[derive(Clone, Default)]
struct AppContext {
    name: Option<String>,
    icon_url: Option<String>,
}

fn handle_element(
    element: &XmlElementEvent,
    inherited_app: Option<&AppContext>,
    actions: &mut Vec<WopiDiscoveryAction>,
    proof_keys: &mut Option<WopiProofKeySet>,
) -> Result<AppContext> {
    let name = element.local_name();
    let context = if name.eq_ignore_ascii_case("app") {
        AppContext {
            name: element
                .attribute("name")
                .map(str::to_string)
                .or_else(|| inherited_app.and_then(|context| context.name.clone())),
            icon_url: element
                .attribute("favIconUrl")
                .map(str::to_string)
                .or_else(|| inherited_app.and_then(|context| context.icon_url.clone())),
        }
    } else {
        inherited_app.cloned().unwrap_or_default()
    };

    if name.eq_ignore_ascii_case("proof-key") {
        let modulus = element
            .attribute("modulus")
            .ok_or_else(|| AsterError::validation_error("WOPI proof-key is missing modulus"))?;
        let exponent = element
            .attribute("exponent")
            .ok_or_else(|| AsterError::validation_error("WOPI proof-key is missing exponent"))?;
        let parsed = parse_wopi_proof_key_set(
            modulus,
            exponent,
            element.attribute("oldmodulus"),
            element.attribute("oldexponent"),
        )?;
        if proof_keys.replace(parsed).is_some() {
            return Err(AsterError::validation_error(
                "WOPI discovery contains multiple proof-key elements",
            ));
        }
    }

    if name.eq_ignore_ascii_case("action") {
        let action = element
            .attribute("name")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase);
        let urlsrc = element
            .attribute("urlsrc")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if let (Some(action), Some(urlsrc)) = (action, urlsrc) {
            actions.push(WopiDiscoveryAction {
                action,
                app_icon_url: context
                    .icon_url
                    .as_deref()
                    .map(str::trim)
                    .map(ToString::to_string),
                app_name: context
                    .name
                    .as_deref()
                    .map(str::trim)
                    .map(ToString::to_string),
                ext: element
                    .attribute("ext")
                    .map(str::trim)
                    .map(|value| value.trim_start_matches('.').to_ascii_lowercase())
                    .filter(|value| !value.is_empty()),
                mime: context
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| value.contains('/'))
                    .map(str::to_ascii_lowercase),
                urlsrc,
            });
        }
    }

    Ok(context)
}
