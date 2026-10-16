use super::*;
use hal_impl::PAGE_SIZE;
use lock::Mutex;
use zircon_object::dev::*;
use zircon_object::task::Job;
use zircon_object::vm::{pages, VmObject};

/// Global sampler session state. Only one session at a time.
static SAMPLER_SESSION: Mutex<Option<SamplerSession>> = Mutex::new(None);

/// Tracks a sampler session's VMO buffer and state.
struct SamplerSession {
    /// VMO buffer for sample records.
    buffer: alloc::sync::Arc<VmObject>,
    /// Buffer size in bytes.
    buffer_size: usize,
    /// Sampling period in nanoseconds.
    _period_ns: u64,
    /// Whether sampling is currently active.
    active: bool,
    /// Write offset (bytes written to the buffer so far).
    write_offset: usize,
    /// The root job at the time the sampler was created. Used to
    /// enumerate all threads for sampling.
    root_job: alloc::sync::Arc<Job>,
}

/// Minimum sampling period (10 microseconds).
const SAMPLER_MIN_PERIOD_NS: u64 = 10_000;

/// Size of zx_sampler_config_t: period(8) + buffer_size(8).
const SAMPLER_CONFIG_SIZE: usize = 16;

/// FXT record type for profiler samples.
const FXT_RECORD_TYPE_PROFILER: u64 = 8;

/// Build an FXT profiler record header.
/// Format: bits 0-3 = type, bits 4-15 = record size (in 8-byte words).
fn fxt_profiler_header(size_words: u64) -> u64 {
    FXT_RECORD_TYPE_PROFILER | (size_words << 4)
}

/// Write a sample record for each running thread into the buffer.
/// Returns the number of bytes written.
fn sample_all_threads(
    root_job: &Job,
    buffer: &VmObject,
    buffer_size: usize,
    offset: usize,
) -> usize {
    let threads = root_job.get_all_threads();
    let now = hal_impl::timer::timer_now().as_nanos() as u64;
    let mut written = 0usize;

    for thread in &threads {
        let pid = thread.proc().id();
        let tid = thread.id();

        // FXT profiler record: 4 words (32 bytes).
        // word 0: header (type=8, size=4)
        // word 1: timestamp
        // word 2: pid
        // word 3: tid
        let record_size = 4 * 8;
        if offset + written + record_size > buffer_size {
            break; // buffer full
        }

        let header = fxt_profiler_header(4);
        let mut record = [0u8; 32];
        record[0..8].copy_from_slice(&header.to_ne_bytes());
        record[8..16].copy_from_slice(&now.to_ne_bytes());
        record[16..24].copy_from_slice(&pid.to_ne_bytes());
        record[24..32].copy_from_slice(&tid.to_ne_bytes());

        if buffer.write(offset + written, &record).is_ok() {
            written += record_size;
        }
    }
    written
}

impl Syscall<'_> {
    /// Create a thread sampler session.
    ///
    /// Parses `zx_sampler_config_t`, creates a VMO buffer,
    /// and registers a global sampler session.
    /// Returns a handle to the buffer VMO (used as the session handle).
    pub fn sys_sampler_create(
        &self,
        resource: HandleValue,
        options: u64,
        config_ptr: usize,
        mut handle_out: UserOutPtr<HandleValue>,
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
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_SAMPLING_BASE, 1)?;
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
        ) as usize;

        if period_ns < SAMPLER_MIN_PERIOD_NS || buffer_size == 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Only one session at a time.  If the previous session's VMO
        // has been dropped (handle closed), clear the stale session.
        let mut session = SAMPLER_SESSION.lock();
        if let Some(existing) = session.as_ref() {
            // Check if the VMO is still alive (strong count > 1 means
            // someone still holds a handle to it).
            if alloc::sync::Arc::strong_count(&existing.buffer) > 1 {
                return Err(ZxError::ALREADY_EXISTS);
            }
            // VMO was dropped — clear the stale session.
            *session = None;
        }

        let region_pages = pages(buffer_size);
        let vmo = VmObject::new_paged(region_pages);
        vmo.set_name("sampler-buffer");

        // Walk up to the root job so we can enumerate all threads.
        let mut root = proc.job();
        while let Some(parent) = root.parent() {
            root = parent;
        }

        *session = Some(SamplerSession {
            buffer: vmo.clone(),
            buffer_size: region_pages * PAGE_SIZE,
            _period_ns: period_ns,
            active: false,
            write_offset: 0,
            root_job: root,
        });

        let handle = proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
        handle_out.write(handle)?;
        Ok(())
    }

    /// Start periodic sampling.
    ///
    /// In libos mode, sampling is done eagerly at read time rather than
    /// via periodic timer interrupts. This just marks the session active.
    pub fn sys_sampler_start(&self, handle: HandleValue) -> ZxResult {
        info!("sampler.start: handle={:#x}", handle);
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle, Rights::WRITE)?;

        let mut session = SAMPLER_SESSION.lock();
        let sess = session.as_mut().ok_or(ZxError::BAD_STATE)?;
        if vmo.id() != sess.buffer.id() {
            return Err(ZxError::ACCESS_DENIED);
        }
        if sess.active {
            return Err(ZxError::BAD_STATE);
        }
        sess.active = true;

        // Take an initial sample of all threads.
        let written = sample_all_threads(
            &sess.root_job,
            &sess.buffer,
            sess.buffer_size,
            sess.write_offset,
        );
        sess.write_offset += written;

        info!(
            "sampler: started, wrote {} bytes of initial samples",
            written
        );
        Ok(())
    }

    /// Stop periodic sampling.
    pub fn sys_sampler_stop(&self, handle: HandleValue) -> ZxResult {
        info!("sampler.stop: handle={:#x}", handle);
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle, Rights::WRITE)?;

        let mut session = SAMPLER_SESSION.lock();
        let sess = session.as_mut().ok_or(ZxError::BAD_STATE)?;
        if vmo.id() != sess.buffer.id() {
            return Err(ZxError::ACCESS_DENIED);
        }
        if !sess.active {
            return Err(ZxError::BAD_STATE);
        }

        // Take a final sample before stopping.
        let written = sample_all_threads(
            &sess.root_job,
            &sess.buffer,
            sess.buffer_size,
            sess.write_offset,
        );
        sess.write_offset += written;

        sess.active = false;
        info!("sampler: stopped, total {} bytes", sess.write_offset);
        Ok(())
    }

    /// Read available samples from the sampler buffer.
    ///
    /// If `data` is null and `data_size` is 0, returns the total
    /// available data size in `actual` (for size query).
    pub fn sys_sampler_read(
        &self,
        handle: HandleValue,
        mut data: UserOutPtr<u8>,
        data_size: usize,
        mut actual: UserOutPtr<usize>,
    ) -> ZxResult {
        info!("sampler.read: handle={:#x}, size={}", handle, data_size);
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle, Rights::READ)?;

        let mut session = SAMPLER_SESSION.lock();
        let sess = session.as_mut().ok_or(ZxError::BAD_STATE)?;
        if vmo.id() != sess.buffer.id() {
            return Err(ZxError::ACCESS_DENIED);
        }

        // If sampling is active, take more samples before reading.
        if sess.active {
            let written = sample_all_threads(
                &sess.root_job,
                &sess.buffer,
                sess.buffer_size,
                sess.write_offset,
            );
            sess.write_offset += written;
        }

        let total_available = sess.write_offset;

        if data.is_null() || data_size == 0 {
            // Size query.
            actual.write(total_available)?;
            return Ok(());
        }

        let to_read = core::cmp::min(total_available, data_size);
        if to_read > 0 {
            let mut buf = alloc::vec![0u8; to_read];
            sess.buffer.read(0, &mut buf)?;
            data.write_array(&buf)?;
        }
        actual.write(to_read)?;
        Ok(())
    }
}

/// When the handle to the sampler VMO is closed, clear the session
/// so a new one can be created. This is detected when the VMO's
/// last handle is closed (the test drops the handle).
impl Drop for SamplerSession {
    fn drop(&mut self) {
        // Session is being dropped — nothing special needed.
    }
}
