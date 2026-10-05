use super::*;
use lock::Mutex;
use zircon_object::dev::*;
use zircon_object::ipc::{IoBuffer, IoBufferRegion};
use zircon_object::vm::{pages, VmObject};

/// Global sampler session state. Only one session at a time.
static SAMPLER_SESSION: Mutex<Option<SamplerSession>> = Mutex::new(None);

/// Tracks a sampler session's IOB and state.
struct SamplerSession {
    /// KoID of the IOBuffer created for this session.
    iob_id: KoID,
    /// Whether sampling is currently active.
    active: bool,
    /// Sampling period in nanoseconds.
    period_ns: u64,
    /// Per-CPU write offsets (bytes written to each region).
    write_offsets: alloc::vec::Vec<usize>,
}

/// Minimum sampling period (10 microseconds).
const SAMPLER_MIN_PERIOD_NS: u64 = 10_000;

/// Size of zx_sampler_config_t: period(8) + buffer_size(8) + discipline(8).
const SAMPLER_CONFIG_SIZE: usize = 24;

/// Number of per-CPU regions to create. Use the compile-time max.
const NUM_SAMPLER_CPUS: usize = hal_impl::config::MAX_CORE_NUM;

impl Syscall<'_> {
    /// Create a thread sampler session.
    ///
    /// Parses `zx_sampler_config_t`, creates an IOBuffer with one
    /// region per CPU, and registers a global sampler session.
    /// Returns the IOBuffer handle.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_sampler_create(
        &self,
        resource: HandleValue,
        options: u64,
        config_ptr: usize,
        config_size: usize,
        mut iob_out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "sampler.create: resource={:#x}, options={}",
            resource, options
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let res = proc.get_resource(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_DEBUG_BASE, 1)?;
        }

        if config_size < SAMPLER_CONFIG_SIZE {
            return Err(ZxError::INVALID_ARGS);
        }
        let config_buf: UserInPtr<u8> = config_ptr.into();
        let config_data = config_buf.read_array(SAMPLER_CONFIG_SIZE)?;

        let period_ns = u64::from_ne_bytes(
            config_data[0..8]
                .try_into()
                .map_err(|_| ZxError::INVALID_ARGS)?,
        );
        let buffer_size = u64::from_ne_bytes(
            config_data[8..16]
                .try_into()
                .map_err(|_| ZxError::INVALID_ARGS)?,
        );
        let discipline = u64::from_ne_bytes(
            config_data[16..24]
                .try_into()
                .map_err(|_| ZxError::INVALID_ARGS)?,
        );

        if period_ns < SAMPLER_MIN_PERIOD_NS || buffer_size == 0 || discipline != 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Only one session at a time.
        let mut session = SAMPLER_SESSION.lock();
        if session.is_some() {
            return Err(ZxError::ALREADY_EXISTS);
        }

        // Create an IOBuffer with one region per CPU.
        let region_pages = pages(buffer_size as usize);
        let region_size = region_pages * 4096;
        let mut regions = alloc::vec::Vec::with_capacity(NUM_SAMPLER_CPUS);
        for _ in 0..NUM_SAMPLER_CPUS {
            let vmo = VmObject::new_paged(region_pages);
            vmo.set_name("sampler-region");
            regions.push(IoBufferRegion::new(vmo, region_size, 0xFF));
        }

        let (ep0, _ep1) = IoBuffer::create(regions)?;
        let iob_id = ep0.id();

        *session = Some(SamplerSession {
            iob_id,
            active: false,
            period_ns,

            write_offsets: alloc::vec![0usize; NUM_SAMPLER_CPUS],
        });

        let handle = proc.add_handle(Handle::new(ep0, Rights::DEFAULT_IOB));
        iob_out.write(handle)?;
        Ok(())
    }

    /// Start periodic sampling.
    pub fn sys_sampler_start(&self, handle: HandleValue) -> ZxResult {
        info!("sampler.start: handle={:#x}", handle);
        let proc = self.thread.proc();
        let iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;

        let mut session = SAMPLER_SESSION.lock();
        let sess = session.as_mut().ok_or(ZxError::BAD_STATE)?;
        if sess.iob_id != iob.id() {
            return Err(ZxError::ACCESS_DENIED);
        }
        if sess.active {
            return Err(ZxError::BAD_STATE);
        }
        sess.active = true;
        // Per-CPU timer arming for PC capture is a follow-up.
        // The session is marked active; samples will be collected
        // when timer_tick hooks are added.
        info!("sampler: started, period={}ns", sess.period_ns);
        Ok(())
    }

    /// Stop periodic sampling.
    pub fn sys_sampler_stop(&self, handle: HandleValue) -> ZxResult {
        info!("sampler.stop: handle={:#x}", handle);
        let proc = self.thread.proc();
        let iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;

        let mut session = SAMPLER_SESSION.lock();
        let sess = session.as_mut().ok_or(ZxError::BAD_STATE)?;
        if sess.iob_id != iob.id() {
            return Err(ZxError::ACCESS_DENIED);
        }
        if !sess.active {
            return Err(ZxError::BAD_STATE);
        }
        sess.active = false;
        info!("sampler: stopped");
        Ok(())
    }

    /// Read available samples from a stopped sampler.
    pub fn sys_sampler_read(
        &self,
        handle: HandleValue,
        mut data: UserOutPtr<u8>,
        data_size: usize,
        mut actual: UserOutPtr<usize>,
    ) -> ZxResult {
        info!("sampler.read: handle={:#x}, size={}", handle, data_size);
        let proc = self.thread.proc();
        let iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::READ)?;

        let session = SAMPLER_SESSION.lock();
        let sess = session.as_ref().ok_or(ZxError::BAD_STATE)?;
        if sess.iob_id != iob.id() {
            return Err(ZxError::ACCESS_DENIED);
        }
        if sess.active {
            return Err(ZxError::BAD_STATE);
        }

        // Read from each per-CPU region into the output buffer.
        let mut bytes_read = 0usize;
        for (i, &offset) in sess.write_offsets.iter().enumerate() {
            if offset == 0 || bytes_read >= data_size {
                continue;
            }
            let to_read = core::cmp::min(offset, data_size - bytes_read);
            let (vmo, _, _) = iob.get_region(i)?;
            let mut buf = alloc::vec![0u8; to_read];
            vmo.read(0, &mut buf)?;
            data.write_array(&buf)?;
            bytes_read += to_read;
        }
        actual.write(bytes_read)?;
        Ok(())
    }
}
