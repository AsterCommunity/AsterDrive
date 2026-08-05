//! AsterDrive product capability projection for the Forge WebDAV protocol engine.

use aster_forge_webdav::{
    DavBackendError, DavCapabilityContext, DavCapabilityDeclaration, DavCapabilityProvider,
    DavCapabilitySnapshot, DavCapabilityTarget, DavClass1Support, DavClass2Profile,
    DavClass2Support, DavCompatibilityCapabilities, DavComplianceClasses, DavExtensionPackage,
    DavExtensionSet, DavLockingCapability, DavMethod, DavMethodSet, DavQuotaExtension,
    DavQuotaSupport, DavResourceState, dav_capability_profile, plan_capabilities,
};

use crate::webdav::backend::AsterDavFs;

pub(crate) struct DriveDavCapabilityProvider<'a> {
    filesystem: &'a AsterDavFs,
}

impl<'a> DriveDavCapabilityProvider<'a> {
    pub(crate) const fn new(filesystem: &'a AsterDavFs) -> Self {
        Self { filesystem }
    }

    pub(crate) fn declaration_for(resource: DavResourceState) -> DavCapabilityDeclaration {
        let methods = match resource {
            DavResourceState::MountRoot => &[
                DavMethod::Options,
                DavMethod::Propfind,
                DavMethod::Lock,
                DavMethod::Unlock,
            ][..],
            DavResourceState::Collection => &[
                DavMethod::Options,
                DavMethod::Delete,
                DavMethod::Copy,
                DavMethod::Move,
                DavMethod::Propfind,
                DavMethod::Proppatch,
                DavMethod::Lock,
                DavMethod::Unlock,
            ],
            DavResourceState::File => &[
                DavMethod::Options,
                DavMethod::Get,
                DavMethod::Put,
                DavMethod::Delete,
                DavMethod::Copy,
                DavMethod::Move,
                DavMethod::Propfind,
                DavMethod::Proppatch,
                DavMethod::Lock,
                DavMethod::Unlock,
            ],
            DavResourceState::Unmapped => &[
                DavMethod::Options,
                DavMethod::Put,
                DavMethod::Mkcol,
                DavMethod::Lock,
                DavMethod::Unlock,
            ],
            DavResourceState::Principal
            | DavResourceState::RedirectReference
            | DavResourceState::AddMemberEndpoint => &[DavMethod::Options],
        };
        let mut declaration =
            DavCapabilityDeclaration::new(resource, DavMethodSet::from_methods(methods));
        declaration.locking = if matches!(
            resource,
            DavResourceState::Principal
                | DavResourceState::RedirectReference
                | DavResourceState::AddMemberEndpoint
        ) {
            DavLockingCapability::Disabled
        } else {
            DavLockingCapability::Class2
        };
        declaration.compliance = DavComplianceClasses {
            class1: true,
            class3: false,
        };
        declaration.compatibility = DavCompatibilityCapabilities {
            ms_author_via: true,
        };
        if matches!(
            resource,
            DavResourceState::MountRoot | DavResourceState::Collection
        ) {
            declaration.extensions = DavExtensionSet::from_packages(&[DavExtensionPackage::Quota]);
        }
        declaration
    }

    pub(crate) fn snapshot_for(
        resource: DavResourceState,
    ) -> Result<DavCapabilitySnapshot, aster_forge_webdav::DavCapabilityPlanError> {
        plan_capabilities(Self::declaration_for(resource))
    }
}

impl DavClass1Support for DriveDavCapabilityProvider<'_> {}
impl DavClass2Support for DriveDavCapabilityProvider<'_> {}
impl DavQuotaSupport for DriveDavCapabilityProvider<'_> {}

impl DavCapabilityProvider for DriveDavCapabilityProvider<'_> {
    type Profile = dav_capability_profile!(DavClass2Profile; DavQuotaExtension);

    async fn capabilities(
        &self,
        target: &DavCapabilityTarget,
        _context: &DavCapabilityContext,
    ) -> Result<DavCapabilityDeclaration, DavBackendError> {
        let resource = self
            .filesystem
            .capability_resource_state(&target.path)
            .await?;
        Ok(Self::declaration_for(resource))
    }
}

#[cfg(test)]
mod tests {
    use super::DriveDavCapabilityProvider;
    use aster_forge_webdav::{
        DavExtensionPackage, DavLockingCapability, DavMethod, DavMethodSet, DavResourceState,
    };

    #[test]
    fn declarations_and_snapshots_match_each_resource_state() {
        let expected = [
            (
                DavResourceState::MountRoot,
                &[
                    DavMethod::Options,
                    DavMethod::Propfind,
                    DavMethod::Lock,
                    DavMethod::Unlock,
                ][..],
                true,
                DavLockingCapability::Class2,
            ),
            (
                DavResourceState::Collection,
                &[
                    DavMethod::Options,
                    DavMethod::Delete,
                    DavMethod::Copy,
                    DavMethod::Move,
                    DavMethod::Propfind,
                    DavMethod::Proppatch,
                    DavMethod::Lock,
                    DavMethod::Unlock,
                ],
                true,
                DavLockingCapability::Class2,
            ),
            (
                DavResourceState::File,
                &[
                    DavMethod::Options,
                    DavMethod::Get,
                    DavMethod::Put,
                    DavMethod::Delete,
                    DavMethod::Copy,
                    DavMethod::Move,
                    DavMethod::Propfind,
                    DavMethod::Proppatch,
                    DavMethod::Lock,
                    DavMethod::Unlock,
                ],
                false,
                DavLockingCapability::Class2,
            ),
            (
                DavResourceState::Unmapped,
                &[
                    DavMethod::Options,
                    DavMethod::Put,
                    DavMethod::Mkcol,
                    DavMethod::Lock,
                    DavMethod::Unlock,
                ],
                false,
                DavLockingCapability::Class2,
            ),
            (
                DavResourceState::Principal,
                &[DavMethod::Options],
                false,
                DavLockingCapability::Disabled,
            ),
            (
                DavResourceState::RedirectReference,
                &[DavMethod::Options],
                false,
                DavLockingCapability::Disabled,
            ),
            (
                DavResourceState::AddMemberEndpoint,
                &[DavMethod::Options],
                false,
                DavLockingCapability::Disabled,
            ),
        ];

        for (resource, methods, supports_quota, locking) in expected {
            let declaration = DriveDavCapabilityProvider::declaration_for(resource);
            assert_eq!(declaration.methods, DavMethodSet::from_methods(methods));
            assert_eq!(declaration.locking, locking);
            assert!(declaration.compliance.class1);
            assert!(!declaration.compliance.class3);
            assert_eq!(
                declaration.extensions.contains(DavExtensionPackage::Quota),
                supports_quota
            );

            let snapshot = DriveDavCapabilityProvider::snapshot_for(resource).unwrap();
            assert_eq!(snapshot.declaration().resource, resource);
            assert!(declaration.methods.is_subset_of(snapshot.methods()));
            assert_eq!(
                snapshot.supports_extension(DavExtensionPackage::Quota),
                supports_quota
            );
        }
    }
}
