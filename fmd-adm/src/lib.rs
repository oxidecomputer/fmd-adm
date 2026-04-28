pub use illumos_nvpair::{NvList, NvValue};

use std::ffi::{CStr, CString};
use std::os::raw::c_void;

use uuid::Uuid;

use fmd_adm_sys::{
    FMD_ADM_MOD_FAILED, FMD_ADM_PROGRAM, FMD_ADM_RSRC_FAULTY, FMD_ADM_RSRC_INVISIBLE,
    FMD_ADM_RSRC_UNUSABLE, FMD_ADM_SERD_FIRED, FMD_ADM_VERSION, FMD_TYPE_BOOL, FMD_TYPE_INT32,
    FMD_TYPE_INT64, FMD_TYPE_SIZE, FMD_TYPE_STRING, FMD_TYPE_TIME, FMD_TYPE_UINT32,
    FMD_TYPE_UINT64, fmd_adm_caseinfo_t, fmd_adm_close, fmd_adm_errmsg, fmd_adm_modinfo_t,
    fmd_adm_rsrcinfo_t, fmd_adm_serdinfo_t, fmd_adm_stats_t, fmd_adm_t, fmd_stat_t,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to open fmd adm handle")]
    Open,
    #[error("fmd: {0}")]
    Fmd(String),
    #[error("interior nul byte in string argument")]
    Nul(#[from] std::ffi::NulError),
    #[error("invalid UUID from fmd: {0}")]
    Uuid(#[from] uuid::Error),
}

/// Whether to include resources flagged as invisible when listing or counting.
///
/// FMD marks some resources as "invisible" — they exist in the daemon's
/// internal model but aren't exposed to typical administrative tools. Callers
/// usually want only directly-visible resources; pass `Included` to also
/// surface the invisible ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvisibleResources {
    Included,
    Excluded,
}

impl InvisibleResources {
    fn as_c_int(self) -> std::os::raw::c_int {
        match self {
            InvisibleResources::Included => 1,
            InvisibleResources::Excluded => 0,
        }
    }
}

/// A handle to the Fault Management Daemon administrative interface.
///
/// This handle wraps a C `fmd_adm_t` pointer that is not thread-safe.
/// `FmdAdm` is `!Send` and `!Sync` — it cannot be shared across threads.
///
/// All iterator methods (`modules()`, `cases()`, `resources()`, etc.)
/// eagerly collect results into a `Vec`. The underlying C API uses
/// callbacks that own the data only for the duration of each
/// invocation, so results must be copied before the callback returns.
pub struct FmdAdm {
    handle: *mut fmd_adm_t,
}

/// Safely convert a C string pointer to an owned `String`.
/// Returns an empty string if the pointer is null.
///
/// # Safety
///
/// If non-null, `p` must point to a valid, nul-terminated C string.
unsafe fn cstr_to_owned(p: *const std::os::raw::c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl FmdAdm {
    /// Open a connection to the local fault management daemon.
    pub fn open() -> Result<Self, Error> {
        let handle = unsafe {
            fmd_adm_sys::fmd_adm_open(std::ptr::null(), FMD_ADM_PROGRAM, FMD_ADM_VERSION as i32)
        };
        if handle.is_null() {
            return Err(Error::Open);
        }
        Ok(Self { handle })
    }

    fn errmsg(&self) -> String {
        let p = unsafe { fmd_adm_errmsg(self.handle) };
        if p.is_null() {
            "unknown error".to_string()
        } else {
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }

    /// Iterate over loaded FMD modules.
    pub fn modules(&self) -> Result<Vec<ModuleInfo>, Error> {
        let mut results: Vec<ModuleInfo> = Vec::new();

        unsafe extern "C" fn callback(
            info: *const fmd_adm_modinfo_t,
            arg: *mut c_void,
        ) -> std::os::raw::c_int {
            unsafe {
                let vec = &mut *(arg as *mut Vec<ModuleInfo>);
                let info = &*info;
                vec.push(ModuleInfo {
                    name: cstr_to_owned(info.ami_name),
                    description: cstr_to_owned(info.ami_desc),
                    version: cstr_to_owned(info.ami_vers),
                    failed: (info.ami_flags & FMD_ADM_MOD_FAILED) != 0,
                });
                0
            }
        }

        let rc = unsafe {
            fmd_adm_sys::fmd_adm_module_iter(
                self.handle,
                Some(callback),
                &mut results as *mut _ as *mut c_void,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(results)
    }

    /// Iterate over faulty resources.
    ///
    /// `include` controls whether resources flagged as invisible are surfaced
    /// alongside directly-visible ones.
    pub fn resources(&self, include: InvisibleResources) -> Result<Vec<ResourceInfo>, Error> {
        // Collect raw strings from the callback, then parse UUIDs
        // afterwards so we can propagate errors.
        struct RawResourceInfo {
            fmri: String,
            uuid: String,
            case: String,
            flags: u32,
        }

        let mut raw: Vec<RawResourceInfo> = Vec::new();

        unsafe extern "C" fn callback(
            info: *const fmd_adm_rsrcinfo_t,
            arg: *mut c_void,
        ) -> std::os::raw::c_int {
            unsafe {
                let vec = &mut *(arg as *mut Vec<RawResourceInfo>);
                let info = &*info;
                vec.push(RawResourceInfo {
                    fmri: cstr_to_owned(info.ari_fmri),
                    uuid: cstr_to_owned(info.ari_uuid),
                    case: cstr_to_owned(info.ari_case),
                    flags: info.ari_flags,
                });
                0
            }
        }

        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_iter(
                self.handle,
                include.as_c_int(),
                Some(callback),
                &mut raw as *mut _ as *mut c_void,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }

        raw.into_iter()
            .map(|r| {
                Ok(ResourceInfo {
                    fmri: r.fmri,
                    uuid: r.uuid.parse()?,
                    case: r.case.parse()?,
                    faulty: (r.flags & FMD_ADM_RSRC_FAULTY) != 0,
                    unusable: (r.flags & FMD_ADM_RSRC_UNUSABLE) != 0,
                    invisible: (r.flags & FMD_ADM_RSRC_INVISIBLE) != 0,
                })
            })
            .collect()
    }

    /// Get the count of faulty resources.
    pub fn resource_count(&self, include: InvisibleResources) -> Result<u32, Error> {
        let mut count: u32 = 0;
        let rc =
            unsafe { fmd_adm_sys::fmd_adm_rsrc_count(self.handle, include.as_c_int(), &mut count) };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(count)
    }

    /// Mark a resource (by FMRI) as repaired.
    pub fn resource_repaired(&mut self, fmri: &str) -> Result<(), Error> {
        let fmri = CString::new(fmri)?;
        let rc = unsafe { fmd_adm_sys::fmd_adm_rsrc_repaired(self.handle, fmri.as_ptr()) };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Mark a resource (by FMRI) as replaced.
    pub fn resource_replaced(&mut self, fmri: &str) -> Result<(), Error> {
        let fmri = CString::new(fmri)?;
        let rc = unsafe { fmd_adm_sys::fmd_adm_rsrc_replaced(self.handle, fmri.as_ptr()) };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Acquit a resource (by FMRI) for a specific case UUID.
    pub fn resource_acquit(&mut self, fmri: &str, case_uuid: &Uuid) -> Result<(), Error> {
        let fmri = CString::new(fmri)?;
        let uuid_c = CString::new(case_uuid.to_string())?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_acquit(self.handle, fmri.as_ptr(), uuid_c.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Iterate over cases, optionally filtered by URL.
    pub fn cases(&self, url: Option<&str>) -> Result<Vec<CaseInfo>, Error> {
        struct RawCaseInfo {
            uuid: String,
            code: String,
            url: String,
            event: Option<NvList>,
        }

        let mut raw: Vec<RawCaseInfo> = Vec::new();
        let url_c = url.map(CString::new).transpose()?;

        unsafe extern "C" fn callback(
            info: *const fmd_adm_caseinfo_t,
            arg: *mut c_void,
        ) -> std::os::raw::c_int {
            unsafe {
                let vec = &mut *(arg as *mut Vec<RawCaseInfo>);
                let info = &*info;
                let event = if info.aci_event.is_null() {
                    None
                } else {
                    // SAFETY: from_raw borrows the nvlist and deep-copies
                    // all values into owned Rust types. It does not take
                    // ownership or free the pointer — fmd_adm_case_iter
                    // remains responsible for calling nvlist_free after
                    // this callback returns.
                    NvList::from_raw(info.aci_event.cast()).ok()
                };
                vec.push(RawCaseInfo {
                    uuid: cstr_to_owned(info.aci_uuid),
                    code: cstr_to_owned(info.aci_code),
                    url: cstr_to_owned(info.aci_url),
                    event,
                });
                0
            }
        }

        let url_ptr = url_c
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null());

        let rc = unsafe {
            fmd_adm_sys::fmd_adm_case_iter(
                self.handle,
                url_ptr,
                Some(callback),
                &mut raw as *mut _ as *mut c_void,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }

        raw.into_iter()
            .map(|r| {
                Ok(CaseInfo {
                    uuid: r.uuid.parse()?,
                    code: r.code,
                    url: r.url,
                    event: r.event,
                })
            })
            .collect()
    }

    /// Acquit a case by UUID.
    pub fn case_acquit(&mut self, uuid: &Uuid) -> Result<(), Error> {
        let uuid = CString::new(uuid.to_string())?;
        let rc = unsafe { fmd_adm_sys::fmd_adm_case_acquit(self.handle, uuid.as_ptr()) };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Iterate over SERD engines for a module.
    pub fn serd_engines(&self, module: &str) -> Result<Vec<SerdInfo>, Error> {
        let mut results: Vec<SerdInfo> = Vec::new();
        let module = CString::new(module)?;

        unsafe extern "C" fn callback(
            info: *const fmd_adm_serdinfo_t,
            arg: *mut c_void,
        ) -> std::os::raw::c_int {
            unsafe {
                let vec = &mut *(arg as *mut Vec<SerdInfo>);
                let info = &*info;
                vec.push(SerdInfo {
                    name: cstr_to_owned(info.asi_name),
                    delta_ns: info.asi_delta,
                    n: info.asi_n,
                    t_ns: info.asi_t,
                    count: info.asi_count,
                    fired: (info.asi_flags & FMD_ADM_SERD_FIRED) != 0,
                });
                0
            }
        }

        let rc = unsafe {
            fmd_adm_sys::fmd_adm_serd_iter(
                self.handle,
                module.as_ptr(),
                Some(callback),
                &mut results as *mut _ as *mut c_void,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(results)
    }

    /// Iterate over transports.
    pub fn transports(&self) -> Result<Vec<TransportId>, Error> {
        let mut results: Vec<TransportId> = Vec::new();

        unsafe extern "C" fn callback(id: fmd_adm_sys::id_t, arg: *mut c_void) {
            unsafe {
                let vec = &mut *(arg as *mut Vec<TransportId>);
                vec.push(TransportId(id));
            }
        }

        let rc = unsafe {
            fmd_adm_sys::fmd_adm_xprt_iter(
                self.handle,
                Some(callback),
                &mut results as *mut _ as *mut c_void,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(results)
    }

    /// Read statistics, optionally for a specific module.
    pub fn stats(&self, module: Option<&str>) -> Result<Vec<Stat>, Error> {
        let module_c = module.map(CString::new).transpose()?;
        let module_ptr = module_c
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null());

        let mut raw_stats = fmd_adm_stats_t {
            ams_buf: std::ptr::null_mut(),
            ams_len: 0,
        };

        let rc =
            unsafe { fmd_adm_sys::fmd_adm_stats_read(self.handle, module_ptr, &mut raw_stats) };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }

        // Ensure stats_free runs even if from_raw or collect panics.
        struct StatsGuard<'a> {
            handle: *mut fmd_adm_t,
            raw: &'a mut fmd_adm_stats_t,
        }
        impl Drop for StatsGuard<'_> {
            fn drop(&mut self) {
                unsafe {
                    fmd_adm_sys::fmd_adm_stats_free(self.handle, self.raw);
                }
            }
        }
        let guard = StatsGuard {
            handle: self.handle,
            raw: &mut raw_stats,
        };

        let len = guard.raw.ams_len as usize;
        let stats = if len == 0 || guard.raw.ams_buf.is_null() {
            Vec::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(guard.raw.ams_buf, len) };
            slice.iter().map(|s| unsafe { Stat::from_raw(s) }).collect()
        };

        // Explicitly drop to free the C buffer before returning.
        drop(guard);

        Ok(stats)
    }
}

impl Drop for FmdAdm {
    fn drop(&mut self) {
        unsafe { fmd_adm_close(self.handle) };
    }
}

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub description: String,
    pub version: String,
    pub failed: bool,
}

#[derive(Debug, Clone)]
pub struct ResourceInfo {
    pub fmri: String,
    pub uuid: Uuid,
    pub case: Uuid,
    pub faulty: bool,
    pub unusable: bool,
    pub invisible: bool,
}

#[derive(Debug, Clone)]
pub struct CaseInfo {
    pub uuid: Uuid,
    pub code: String,
    pub url: String,
    /// The full fault event payload as an nvlist, if present.
    pub event: Option<NvList>,
}

#[derive(Debug, Clone)]
pub struct SerdInfo {
    pub name: String,
    /// Nanoseconds from oldest event to now.
    pub delta_ns: u64,
    /// N parameter (event count threshold).
    pub n: u64,
    /// T parameter (nanoseconds window).
    pub t_ns: u64,
    /// Number of events currently in engine.
    pub count: u32,
    /// Whether the SERD engine has fired.
    pub fired: bool,
}

/// An opaque identifier for an FMD transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransportId(i32);

impl TransportId {
    pub fn as_raw(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for TransportId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub enum StatValue {
    Bool(bool),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    /// A time value in nanoseconds.
    Time(u64),
    /// A size value in bytes.
    Size(u64),
    String(String),
    /// A stat type not recognized by this crate.
    Unknown {
        type_code: u32,
        raw: u64,
    },
}

impl std::fmt::Display for StatValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatValue::Bool(v) => write!(f, "{v}"),
            StatValue::Int32(v) => write!(f, "{v}"),
            StatValue::UInt32(v) => write!(f, "{v}"),
            StatValue::Int64(v) => write!(f, "{v}"),
            StatValue::UInt64(v) => write!(f, "{v}"),
            StatValue::Time(v) => write!(f, "{v}ns"),
            StatValue::Size(v) => write!(f, "{v}B"),
            StatValue::String(v) => write!(f, "{v}"),
            StatValue::Unknown { type_code, raw } => {
                write!(f, "unknown(type={type_code}, raw={raw})")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stat {
    pub name: String,
    pub description: String,
    pub value: StatValue,
}

impl Stat {
    /// Convert a raw `fmd_stat_t` into an owned `Stat`.
    ///
    /// # Safety
    /// The `fmd_stat_t` must have been obtained from a valid fmd call.
    unsafe fn from_raw(raw: &fmd_stat_t) -> Self {
        let name = unsafe { CStr::from_ptr(raw.fmds_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let description = unsafe { CStr::from_ptr(raw.fmds_desc.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let value = match raw.fmds_type {
            FMD_TYPE_BOOL => StatValue::Bool(unsafe { raw.fmds_value.bool_ } != 0),
            FMD_TYPE_INT32 => StatValue::Int32(unsafe { raw.fmds_value.i32_ }),
            FMD_TYPE_UINT32 => StatValue::UInt32(unsafe { raw.fmds_value.ui32 }),
            FMD_TYPE_INT64 => StatValue::Int64(unsafe { raw.fmds_value.i64_ }),
            FMD_TYPE_UINT64 => StatValue::UInt64(unsafe { raw.fmds_value.ui64 }),
            FMD_TYPE_TIME => StatValue::Time(unsafe { raw.fmds_value.ui64 }),
            FMD_TYPE_SIZE => StatValue::Size(unsafe { raw.fmds_value.ui64 }),
            FMD_TYPE_STRING => {
                let p = unsafe { raw.fmds_value.str_ };
                if p.is_null() {
                    StatValue::String(String::new())
                } else {
                    StatValue::String(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
                }
            }
            other => StatValue::Unknown {
                type_code: other,
                raw: unsafe { raw.fmds_value.ui64 },
            },
        };
        Self {
            name,
            description,
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Unit tests (pure logic, no daemon needed) ──

    #[test]
    fn stat_value_display_bool() {
        assert_eq!(StatValue::Bool(true).to_string(), "true");
        assert_eq!(StatValue::Bool(false).to_string(), "false");
    }

    #[test]
    fn stat_value_display_integers() {
        assert_eq!(StatValue::Int32(-42).to_string(), "-42");
        assert_eq!(StatValue::UInt32(42).to_string(), "42");
        assert_eq!(StatValue::Int64(-100).to_string(), "-100");
        assert_eq!(StatValue::UInt64(100).to_string(), "100");
    }

    #[test]
    fn stat_value_display_time() {
        assert_eq!(StatValue::Time(1_000_000_000).to_string(), "1000000000ns");
    }

    #[test]
    fn stat_value_display_size() {
        assert_eq!(StatValue::Size(4096).to_string(), "4096B");
    }

    #[test]
    fn stat_value_display_string() {
        assert_eq!(StatValue::String("hello".into()).to_string(), "hello");
    }

    #[test]
    fn stat_value_display_unknown() {
        let v = StatValue::Unknown {
            type_code: 99,
            raw: 0xdead,
        };
        assert_eq!(v.to_string(), "unknown(type=99, raw=57005)");
    }

    #[test]
    fn transport_id_display() {
        assert_eq!(TransportId(7).to_string(), "7");
    }

    // ── Integration tests (require a running fmd daemon) ──

    #[test]
    fn open_and_close() {
        let _adm = FmdAdm::open().expect("failed to open fmd handle");
    }

    #[test]
    fn list_modules() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let modules = adm.modules().expect("failed to list modules");
        assert!(!modules.is_empty(), "fmd should always have modules loaded");
    }

    #[test]
    fn list_resources() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let _resources = adm
            .resources(InvisibleResources::Excluded)
            .expect("failed to list resources");
        let _all = adm
            .resources(InvisibleResources::Included)
            .expect("failed to list all resources");
    }

    #[test]
    fn resource_count_matches_resources() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let count = adm
            .resource_count(InvisibleResources::Excluded)
            .expect("failed to get resource count");
        let resources = adm
            .resources(InvisibleResources::Excluded)
            .expect("failed to list resources");
        assert_eq!(
            count as usize,
            resources.len(),
            "resource_count should match resources().len()"
        );
    }

    #[test]
    fn list_cases() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let _cases = adm.cases(None).expect("failed to list cases");
    }

    #[test]
    fn list_transports() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let _transports = adm.transports().expect("failed to list transports");
    }

    #[test]
    fn read_global_stats() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let stats = adm.stats(None).expect("failed to read global stats");
        assert!(!stats.is_empty(), "global stats should not be empty");
    }

    #[test]
    fn read_per_module_stats() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let modules = adm.modules().expect("failed to list modules");
        // Read stats for the first module.
        let module = &modules[0];
        let stats = adm
            .stats(Some(&module.name))
            .expect("failed to read module stats");
        assert!(
            !stats.is_empty(),
            "module '{}' should have stats",
            module.name
        );
    }

    #[test]
    fn serd_engines_for_modules() {
        let adm = FmdAdm::open().expect("failed to open fmd handle");
        let modules = adm.modules().expect("failed to list modules");
        // Just verify the call succeeds for each module (most won't have SERD engines).
        for module in &modules {
            let _engines = adm
                .serd_engines(&module.name)
                .expect(&format!("failed to get SERD engines for {}", module.name));
        }
    }
}
