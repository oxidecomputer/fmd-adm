pub use illumos_nvpair::{NvList, NvValue};

use std::ffi::{CStr, CString};
use std::os::raw::c_void;

use uuid::Uuid;

use fmd_adm_sys::{
    fmd_adm_caseinfo_t, fmd_adm_close, fmd_adm_errmsg, fmd_adm_modinfo_t,
    fmd_adm_rsrcinfo_t, fmd_adm_serdinfo_t, fmd_adm_stats_t, fmd_adm_t,
    fmd_stat_t, FMD_ADM_MOD_FAILED, FMD_ADM_PROGRAM, FMD_ADM_RSRC_FAULTY,
    FMD_ADM_RSRC_INVISIBLE, FMD_ADM_RSRC_UNUSABLE, FMD_ADM_SERD_FIRED,
    FMD_ADM_VERSION, FMD_TYPE_BOOL, FMD_TYPE_INT32, FMD_TYPE_INT64,
    FMD_TYPE_SIZE, FMD_TYPE_STRING, FMD_TYPE_TIME, FMD_TYPE_UINT32,
    FMD_TYPE_UINT64,
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

/// A handle to the Fault Management Daemon administrative interface.
pub struct FmdAdm {
    handle: *mut fmd_adm_t,
}

impl FmdAdm {
    /// Open a connection to the local fault management daemon.
    pub fn open() -> Result<Self, Error> {
        let handle = unsafe {
            fmd_adm_sys::fmd_adm_open(
                std::ptr::null(),
                FMD_ADM_PROGRAM,
                FMD_ADM_VERSION as i32,
            )
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
        ) -> std::os::raw::c_int { unsafe {
            let vec = &mut *(arg as *mut Vec<ModuleInfo>);
            let info = &*info;
            vec.push(ModuleInfo {
                name: CStr::from_ptr(info.ami_name)
                    .to_string_lossy()
                    .into_owned(),
                description: CStr::from_ptr(info.ami_desc)
                    .to_string_lossy()
                    .into_owned(),
                version: CStr::from_ptr(info.ami_vers)
                    .to_string_lossy()
                    .into_owned(),
                failed: (info.ami_flags & FMD_ADM_MOD_FAILED) != 0,
            });
            0
        }}

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

    /// Load an FMD module by path.
    pub fn module_load(&self, path: &str) -> Result<(), Error> {
        let path = CString::new(path)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_module_load(self.handle, path.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Unload an FMD module by name.
    pub fn module_unload(&self, name: &str) -> Result<(), Error> {
        let name = CString::new(name)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_module_unload(self.handle, name.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Reset an FMD module by name.
    pub fn module_reset(&self, name: &str) -> Result<(), Error> {
        let name = CString::new(name)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_module_reset(self.handle, name.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Garbage-collect an FMD module by name.
    pub fn module_gc(&self, name: &str) -> Result<(), Error> {
        let name = CString::new(name)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_module_gc(self.handle, name.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Iterate over faulty resources.
    ///
    /// If `all` is true, includes resources that are not directly visible.
    pub fn resources(&self, all: bool) -> Result<Vec<ResourceInfo>, Error> {
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
        ) -> std::os::raw::c_int { unsafe {
            let vec = &mut *(arg as *mut Vec<RawResourceInfo>);
            let info = &*info;
            vec.push(RawResourceInfo {
                fmri: CStr::from_ptr(info.ari_fmri)
                    .to_string_lossy()
                    .into_owned(),
                uuid: CStr::from_ptr(info.ari_uuid)
                    .to_string_lossy()
                    .into_owned(),
                case: CStr::from_ptr(info.ari_case)
                    .to_string_lossy()
                    .into_owned(),
                flags: info.ari_flags,
            });
            0
        }}

        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_iter(
                self.handle,
                if all { 1 } else { 0 },
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
    pub fn resource_count(&self, all: bool) -> Result<u32, Error> {
        let mut count: u32 = 0;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_count(
                self.handle,
                if all { 1 } else { 0 },
                &mut count,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(count)
    }

    /// Mark a resource (by FMRI) as repaired.
    pub fn resource_repaired(&self, fmri: &str) -> Result<(), Error> {
        let fmri = CString::new(fmri)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_repaired(self.handle, fmri.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Mark a resource (by FMRI) as replaced.
    pub fn resource_replaced(&self, fmri: &str) -> Result<(), Error> {
        let fmri = CString::new(fmri)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_replaced(self.handle, fmri.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Acquit a resource (by FMRI), optionally specifying a case UUID.
    ///
    /// If `case_uuid` is `None`, the resource is acquitted across all cases.
    pub fn resource_acquit(
        &self,
        fmri: &str,
        case_uuid: Option<&Uuid>,
    ) -> Result<(), Error> {
        let fmri = CString::new(fmri)?;
        let uuid_str = case_uuid.map(|u| CString::new(u.to_string()));
        let uuid_c = uuid_str.transpose()?;
        let uuid_ptr = uuid_c
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null());
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_acquit(
                self.handle,
                fmri.as_ptr(),
                uuid_ptr,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Flush cached state for a resource (by FMRI).
    pub fn resource_flush(&self, fmri: &str) -> Result<(), Error> {
        let fmri = CString::new(fmri)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_rsrc_flush(self.handle, fmri.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Iterate over cases, optionally filtered by URL.
    pub fn cases(
        &self,
        url: Option<&str>,
    ) -> Result<Vec<CaseInfo>, Error> {
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
        ) -> std::os::raw::c_int { unsafe {
            let vec = &mut *(arg as *mut Vec<RawCaseInfo>);
            let info = &*info;
            let event = if info.aci_event.is_null() {
                None
            } else {
                // If from_raw fails, skip the event rather than
                // aborting the entire iteration.
                NvList::from_raw(info.aci_event.cast()).ok()
            };
            vec.push(RawCaseInfo {
                uuid: CStr::from_ptr(info.aci_uuid)
                    .to_string_lossy()
                    .into_owned(),
                code: CStr::from_ptr(info.aci_code)
                    .to_string_lossy()
                    .into_owned(),
                url: CStr::from_ptr(info.aci_url)
                    .to_string_lossy()
                    .into_owned(),
                event,
            });
            0
        }}

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

    /// Repair a case by UUID.
    pub fn case_repair(&self, uuid: &Uuid) -> Result<(), Error> {
        let uuid = CString::new(uuid.to_string())?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_case_repair(self.handle, uuid.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Acquit a case by UUID.
    pub fn case_acquit(&self, uuid: &Uuid) -> Result<(), Error> {
        let uuid = CString::new(uuid.to_string())?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_case_acquit(self.handle, uuid.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Iterate over SERD engines for a module.
    pub fn serd_engines(
        &self,
        module: &str,
    ) -> Result<Vec<SerdInfo>, Error> {
        let mut results: Vec<SerdInfo> = Vec::new();
        let module = CString::new(module)?;

        unsafe extern "C" fn callback(
            info: *const fmd_adm_serdinfo_t,
            arg: *mut c_void,
        ) -> std::os::raw::c_int { unsafe {
            let vec = &mut *(arg as *mut Vec<SerdInfo>);
            let info = &*info;
            vec.push(SerdInfo {
                name: CStr::from_ptr(info.asi_name)
                    .to_string_lossy()
                    .into_owned(),
                delta_ns: info.asi_delta,
                n: info.asi_n,
                t_ns: info.asi_t,
                count: info.asi_count,
                fired: (info.asi_flags & FMD_ADM_SERD_FIRED) != 0,
            });
            0
        }}

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

    /// Reset a SERD engine.
    pub fn serd_reset(
        &self,
        module: &str,
        name: &str,
    ) -> Result<(), Error> {
        let module = CString::new(module)?;
        let name = CString::new(name)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_serd_reset(
                self.handle,
                module.as_ptr(),
                name.as_ptr(),
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
    }

    /// Iterate over transports.
    pub fn transports(&self) -> Result<Vec<TransportId>, Error> {
        let mut results: Vec<TransportId> = Vec::new();

        unsafe extern "C" fn callback(
            id: fmd_adm_sys::id_t,
            arg: *mut c_void,
        ) { unsafe {
            let vec = &mut *(arg as *mut Vec<TransportId>);
            vec.push(TransportId(id));
        }}

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
    pub fn stats(
        &self,
        module: Option<&str>,
    ) -> Result<Vec<Stat>, Error> {
        let module_c = module.map(CString::new).transpose()?;
        let module_ptr = module_c
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null());

        let mut raw_stats = fmd_adm_stats_t {
            ams_buf: std::ptr::null_mut(),
            ams_len: 0,
        };

        let rc = unsafe {
            fmd_adm_sys::fmd_adm_stats_read(
                self.handle,
                module_ptr,
                &mut raw_stats,
            )
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }

        let stats = unsafe {
            let slice = std::slice::from_raw_parts(
                raw_stats.ams_buf,
                raw_stats.ams_len as usize,
            );
            slice.iter().map(|s| Stat::from_raw(s)).collect()
        };

        unsafe {
            fmd_adm_sys::fmd_adm_stats_free(self.handle, &mut raw_stats);
        }

        Ok(stats)
    }

    /// Rotate a log file.
    pub fn log_rotate(&self, log: &str) -> Result<(), Error> {
        let log = CString::new(log)?;
        let rc = unsafe {
            fmd_adm_sys::fmd_adm_log_rotate(self.handle, log.as_ptr())
        };
        if rc != 0 {
            return Err(Error::Fmd(self.errmsg()));
        }
        Ok(())
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
    Unknown { type_code: u32, raw: u64 },
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
    unsafe fn from_raw(raw: &fmd_stat_t) -> Self { unsafe {
        let name = CStr::from_ptr(raw.fmds_name.as_ptr())
            .to_string_lossy()
            .into_owned();
        let description = CStr::from_ptr(raw.fmds_desc.as_ptr())
            .to_string_lossy()
            .into_owned();
        let value = match raw.fmds_type {
            FMD_TYPE_BOOL => StatValue::Bool(raw.fmds_value.bool_ != 0),
            FMD_TYPE_INT32 => StatValue::Int32(raw.fmds_value.i32_),
            FMD_TYPE_UINT32 => StatValue::UInt32(raw.fmds_value.ui32),
            FMD_TYPE_INT64 => StatValue::Int64(raw.fmds_value.i64_),
            FMD_TYPE_UINT64 => StatValue::UInt64(raw.fmds_value.ui64),
            FMD_TYPE_TIME => StatValue::Time(raw.fmds_value.ui64),
            FMD_TYPE_SIZE => StatValue::Size(raw.fmds_value.ui64),
            FMD_TYPE_STRING => {
                if raw.fmds_value.str_.is_null() {
                    StatValue::String(String::new())
                } else {
                    StatValue::String(
                        CStr::from_ptr(raw.fmds_value.str_)
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
            }
            other => StatValue::Unknown {
                type_code: other,
                raw: raw.fmds_value.ui64,
            },
        };
        Self { name, description, value }
    }}
}
