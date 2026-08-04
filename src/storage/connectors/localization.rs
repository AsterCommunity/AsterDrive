//! Composition support for the built-in connector plugins.

use aster_drive_storage::{
    ConnectorId, StorageConnectorDescriptor, StorageConnectorLocalization,
    StorageConnectorLocalizationMessage,
};

use crate::errors::{AsterError, Result};

pub(super) fn builtin_connector_localization(
    connector_id: &'static str,
    descriptor: &StorageConnectorDescriptor,
    connector_messages: &'static [StorageConnectorLocalizationMessage<'static>],
) -> Result<StorageConnectorLocalization> {
    let referenced_message_ids = descriptor.localization_message_ids();
    // Shared helpers contribute only descriptor-referenced messages. A
    // connector's private resource is published in full because its admin UI
    // may need messages for credential state or other connector-owned views
    // that are not themselves descriptor fields.
    StorageConnectorLocalization::from_messages(
        ConnectorId::declared(connector_id),
        "en",
        super::common::LOCALIZATION_MESSAGES
            .iter()
            .filter(|message| referenced_message_ids.contains(message.message_id))
            .chain(connector_messages),
    )
    .map_err(|error| AsterError::internal_error(error.to_string()))
}
