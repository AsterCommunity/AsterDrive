//! AsterDrive product capability projection for the Forge WebDAV protocol engine.

use aster_forge_webdav::{
    DavAutoVersion, DavBackendError, DavCapabilityContext, DavCapabilityDeclaration,
    DavCapabilityProvider, DavCapabilitySnapshot, DavCapabilityTarget, DavClass1Support,
    DavClass2Profile, DavClass2Support, DavCompatibilityCapabilities, DavComplianceClasses,
    DavExtensionPackage, DavExtensionSet, DavLockingCapability, DavMethod, DavMethodSet,
    DavQuotaExtension, DavQuotaSupport, DavResourceState, DavVersionControlExtension,
    DavVersionControlSupport, DavVersioningCapabilities, DavVersioningState,
    dav_capability_profile, plan_capabilities,
};

use crate::webdav::backend::AsterDavFs;

pub(crate) struct DriveDavCapabilityProvider<'a> {
    filesystem: &'a AsterDavFs,
}

impl<'a> DriveDavCapabilityProvider<'a> {
    pub(crate) const fn new(filesystem: &'a AsterDavFs) -> Self {
        Self { filesystem }
    }

    pub(crate) fn declaration_for<T: Into<crate::webdav::backend::DeltavCapabilityTarget>>(
        target: T,
    ) -> DavCapabilityDeclaration {
        let target = target.into();
        let resource = target.resource;
        let methods = if target.reserved_unmapped {
            &[DavMethod::Options][..]
        } else {
            match (resource, target.versioning) {
                (DavResourceState::File, DavVersioningState::Version) => &[
                    DavMethod::Options,
                    DavMethod::Get,
                    DavMethod::Propfind,
                    DavMethod::Report,
                ][..],
                (DavResourceState::MountRoot, _) => &[
                    DavMethod::Options,
                    DavMethod::Propfind,
                    DavMethod::Lock,
                    DavMethod::Unlock,
                ][..],
                (DavResourceState::Collection, _) => &[
                    DavMethod::Options,
                    DavMethod::Delete,
                    DavMethod::Copy,
                    DavMethod::Move,
                    DavMethod::Propfind,
                    DavMethod::Proppatch,
                    DavMethod::Lock,
                    DavMethod::Unlock,
                ],
                (DavResourceState::File, DavVersioningState::Versionable) => &[
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
                    DavMethod::VersionControl,
                ],
                (DavResourceState::File, DavVersioningState::Unsupported) => &[
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
                (DavResourceState::File, _) => &[
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
                    DavMethod::Report,
                    DavMethod::VersionControl,
                ],
                (DavResourceState::Unmapped, _) => &[
                    DavMethod::Options,
                    DavMethod::Put,
                    DavMethod::Mkcol,
                    DavMethod::Lock,
                    DavMethod::Unlock,
                ],
                (
                    DavResourceState::Principal
                    | DavResourceState::RedirectReference
                    | DavResourceState::AddMemberEndpoint,
                    _,
                ) => &[DavMethod::Options],
            }
        };
        let mut declaration =
            DavCapabilityDeclaration::new(resource, DavMethodSet::from_methods(methods));
        declaration.locking = if target.reserved_unmapped
            || target.versioning == DavVersioningState::Version
            || matches!(
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
        if target.versioning != DavVersioningState::Unsupported {
            declaration.extensions =
                declaration
                    .extensions
                    .union(DavExtensionSet::from_packages(&[
                        DavExtensionPackage::VersionControl,
                    ]));
            declaration.versioning = DavVersioningCapabilities {
                state: target.versioning,
                auto_version: if target.versioning == DavVersioningState::CheckedIn {
                    DavAutoVersion::CheckoutCheckin
                } else {
                    DavAutoVersion::None
                },
                write_locked: false,
                auto_checkout_lock: false,
                allow_version_delete: false,
            };
        }
        declaration
    }

    #[cfg(test)]
    pub(crate) fn snapshot_for(
        resource: DavResourceState,
    ) -> Result<DavCapabilitySnapshot, aster_forge_webdav::DavCapabilityPlanError> {
        Self::snapshot_for_versioned(resource, DavVersioningState::Unsupported)
    }

    pub(crate) fn snapshot_for_versioned(
        resource: DavResourceState,
        versioning: DavVersioningState,
    ) -> Result<DavCapabilitySnapshot, aster_forge_webdav::DavCapabilityPlanError> {
        plan_capabilities(Self::declaration_for(
            crate::webdav::backend::DeltavCapabilityTarget {
                resource,
                versioning,
                reserved_unmapped: false,
            },
        ))
    }
}

impl DavClass1Support for DriveDavCapabilityProvider<'_> {}
impl DavClass2Support for DriveDavCapabilityProvider<'_> {}
impl DavQuotaSupport for DriveDavCapabilityProvider<'_> {}
impl DavVersionControlSupport for DriveDavCapabilityProvider<'_> {}

impl DavCapabilityProvider for DriveDavCapabilityProvider<'_> {
    type Profile = dav_capability_profile!(
        DavClass2Profile;
        DavQuotaExtension,
        DavVersionControlExtension
    );

    async fn capabilities(
        &self,
        target: &DavCapabilityTarget,
        _context: &DavCapabilityContext,
    ) -> Result<DavCapabilityDeclaration, DavBackendError> {
        let target = self
            .filesystem
            .deltav_capability_target(&target.path)
            .await?;
        Ok(Self::declaration_for(target))
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
