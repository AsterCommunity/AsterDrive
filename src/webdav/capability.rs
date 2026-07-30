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
                DavMethod::Get,
                DavMethod::Put,
                DavMethod::Delete,
                DavMethod::Copy,
                DavMethod::Move,
                DavMethod::Mkcol,
                DavMethod::Propfind,
                DavMethod::Proppatch,
                DavMethod::Lock,
                DavMethod::Unlock,
            ],
            DavResourceState::Principal
            | DavResourceState::RedirectReference
            | DavResourceState::AddMemberEndpoint => &[DavMethod::Options],
        };
        let mut declaration =
            DavCapabilityDeclaration::new(resource, DavMethodSet::from_methods(methods));
        declaration.locking = DavLockingCapability::Class2;
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
