use {super::*, core::convert::TryFrom, zircon_object::dev::*};

impl Syscall<'_> {
    #[allow(clippy::too_many_arguments)]
    /// Create a resource object for use with other DDK syscalls.
    pub fn sys_resource_create(
        &self,
        parent_rsrc: HandleValue,
        options: u32,
        base: u64,
        size: u64,
        name: UserInPtr<u8>,
        name_size: u64,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        let name = name.read_string(name_size as usize)?;
        info!("name={:?}", name);
        let proc = self.thread.proc();
        let parent_rsrc = proc.get_resource_with_rights(parent_rsrc, Rights::WRITE)?;
        let kind = ResourceKind::try_from(options & 0xFFFF).map_err(|_| ZxError::INVALID_ARGS)?;
        // COUNT and ROOT are not valid kinds for resource_create.
        if kind == ResourceKind::COUNT || kind == ResourceKind::ROOT {
            return Err(ZxError::INVALID_ARGS);
        }
        let flags = ResourceFlags::from_bits(options & 0xFFFF_0000).ok_or(ZxError::INVALID_ARGS)?;
        // Check for arithmetic overflow before range validation.
        if (base as usize).checked_add(size as usize).is_none() {
            return Err(ZxError::INVALID_ARGS);
        }
        // Zero-size non-SYSTEM resources are forbidden.
        if size == 0 && kind != ResourceKind::SYSTEM {
            return Err(ZxError::ACCESS_DENIED);
        }

        // Cannot create children from an exclusive parent resource.
        if parent_rsrc.is_exclusive() {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate the requested range is valid.
        parent_rsrc
            .validate_ranged_resource(kind, base as usize, size as usize)
            .map_err(|e| {
                if e == ZxError::WRONG_TYPE {
                    ZxError::ACCESS_DENIED
                } else {
                    e
                }
            })?;
        // Check for exclusive overlap with existing resources.
        Resource::check_exclusive_overlap(kind, base as usize, size as usize, flags)?;
        let rsrc = Resource::create(&name, kind, base as usize, size as usize, flags);
        rsrc.register_region();
        let handle = proc.add_handle(Handle::new(rsrc, Rights::DEFAULT_RESOURCE));
        out.write(handle)?;
        Ok(())
    }
}
