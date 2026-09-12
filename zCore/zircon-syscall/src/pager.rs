use {
    super::*,
    zircon_object::{signal::*, vm::*},
};

impl Syscall<'_> {
    /// Create a pager object.
    pub fn sys_pager_create(&self, options: u32, mut out: UserOutPtr<HandleValue>) -> ZxResult {
        info!("pager.create: options={:#x}", options);
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let pager = Pager::new();
        let proc = self.thread.proc();
        let handle = proc.add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));
        out.write(handle)?;
        Ok(())
    }

    /// Create a VMO backed by a pager.
    pub fn sys_pager_create_vmo(
        &self,
        pager_handle: HandleValue,
        options: u32,
        port_handle: HandleValue,
        key: u64,
        size: u64,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "pager.create_vmo: pager={:#x}, options={:#x}, port={:#x}, key={:#x}, size={:#x}",
            pager_handle, options, port_handle, key, size
        );
        let proc = self.thread.proc();
        let pager = proc.get_object::<Pager>(pager_handle)?;
        let port = proc.get_object_with_rights::<Port>(port_handle, Rights::WRITE)?;
        let vmo = pager.create_vmo(options, &port, key, size)?;
        let handle = proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
        out.write(handle)?;
        Ok(())
    }

    /// Detach a VMO from its pager.
    pub fn sys_pager_detach_vmo(
        &self,
        pager_handle: HandleValue,
        vmo_handle: HandleValue,
    ) -> ZxResult {
        info!(
            "pager.detach_vmo: pager={:#x}, vmo={:#x}",
            pager_handle, vmo_handle
        );
        let proc = self.thread.proc();
        let pager = proc.get_object::<Pager>(pager_handle)?;
        let vmo = proc.get_object::<VmObject>(vmo_handle)?;
        pager.detach_vmo(&vmo)
    }

    /// Supply pages to a pager-backed VMO from an auxiliary VMO.
    pub fn sys_pager_supply_pages(
        &self,
        pager_handle: HandleValue,
        vmo_handle: HandleValue,
        offset: u64,
        length: u64,
        aux_vmo_handle: HandleValue,
        aux_offset: u64,
    ) -> ZxResult {
        info!(
            "pager.supply_pages: pager={:#x}, vmo={:#x}, offset={:#x}, len={:#x}, aux={:#x}, aux_off={:#x}",
            pager_handle, vmo_handle, offset, length, aux_vmo_handle, aux_offset
        );
        let proc = self.thread.proc();
        let pager = proc.get_object::<Pager>(pager_handle)?;
        let vmo = proc.get_object::<VmObject>(vmo_handle)?;
        let aux_vmo =
            proc.get_object_with_rights::<VmObject>(aux_vmo_handle, Rights::READ | Rights::WRITE)?;
        pager.supply_pages(&vmo, offset, length, &aux_vmo, aux_offset)
    }
}
