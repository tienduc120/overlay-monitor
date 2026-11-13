//! GPU adapters and performance data through the kernel-mode thunks (D3DKMT).

use windows::Wdk::Graphics::Direct3D::{
    D3DKMT_ADAPTER_PERFDATA, D3DKMT_ADAPTERINFO, D3DKMT_ADAPTERREGISTRYINFO, D3DKMT_CLOSEADAPTER, D3DKMT_ENUMADAPTERS2,
    D3DKMT_OPENADAPTERFROMLUID, D3DKMT_QUERYADAPTERINFO, D3DKMT_SEGMENTSIZEINFO, D3DKMTCloseAdapter,
    D3DKMTEnumAdapters2, D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo, KMTQAITYPE_ADAPTERPERFDATA,
    KMTQAITYPE_ADAPTERREGISTRYINFO, KMTQAITYPE_GETSEGMENTSIZE, KMTQUERYADAPTERINFOTYPE,
};
use windows::Win32::Foundation::LUID;

use crate::Win32Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuAdapter {
    /// The adapter's LUID as `(high << 32) | low`, the form the performance counter names use.
    pub luid: u64,
    pub name: String,
    pub dedicated_bytes: u64,
}

/// A kernel adapter handle, closed on drop.
struct Handle(u32);

impl Drop for Handle {
    fn drop(&mut self) {
        let close = D3DKMT_CLOSEADAPTER { hAdapter: self.0 };
        // SAFETY: the pointer comes from a live local, and this handle is closed exactly once, here.
        // A failed close leaves nothing to recover, so its status is ignored.
        unsafe { D3DKMTCloseAdapter(&raw const close) }.ok().ok();
    }
}

/// Fills a `T` from one adapter query.
///
/// `T` must be one of the plain-data query structs, for which all-zero is a valid value.
fn query<T: Default>(adapter: &Handle, kind: KMTQUERYADAPTERINFOTYPE) -> Result<T, Win32Error> {
    let mut data = T::default();
    let mut request = D3DKMT_QUERYADAPTERINFO {
        hAdapter: adapter.0,
        Type: kind,
        pPrivateDriverData: (&raw mut data).cast(),
        PrivateDriverDataSize: size_of::<T>().try_into().unwrap_or(u32::MAX),
    };
    // SAFETY: `request` points at `data`, a live local of exactly `PrivateDriverDataSize` bytes; both outlive the call.
    unsafe { D3DKMTQueryAdapterInfo(&raw mut request) }.ok()?;
    Ok(data)
}

fn luid_value(luid: LUID) -> u64 {
    (u64::from(luid.HighPart.cast_unsigned()) << 32) | u64::from(luid.LowPart)
}

fn describe(adapter: &Handle, luid: LUID) -> Result<GpuAdapter, Win32Error> {
    let registry: D3DKMT_ADAPTERREGISTRYINFO = query(adapter, KMTQAITYPE_ADAPTERREGISTRYINFO)?;
    let segments: D3DKMT_SEGMENTSIZEINFO = query(adapter, KMTQAITYPE_GETSEGMENTSIZE)?;
    let name_len = registry
        .AdapterString
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(registry.AdapterString.len());
    let name = registry
        .AdapterString
        .get(..name_len)
        .map(String::from_utf16_lossy)
        .unwrap_or_default();
    Ok(GpuAdapter {
        luid: luid_value(luid),
        name,
        dedicated_bytes: segments.DedicatedVideoMemorySize,
    })
}

/// Every adapter that answers the name and memory queries; one that does not (such as the basic render driver) is
/// skipped.
pub fn gpu_adapters() -> Result<Vec<GpuAdapter>, Win32Error> {
    let mut request = D3DKMT_ENUMADAPTERS2::default();
    // SAFETY: with a null buffer the call only reports the adapter count in the exclusively borrowed local.
    unsafe { D3DKMTEnumAdapters2(&raw mut request) }.ok()?;
    let mut infos = vec![D3DKMT_ADAPTERINFO::default(); request.NumAdapters as usize];
    request.pAdapters = infos.as_mut_ptr();
    // SAFETY: `pAdapters` points at `NumAdapters` writable entries owned by `infos`, which outlives the call.
    unsafe { D3DKMTEnumAdapters2(&raw mut request) }.ok()?;
    // Own every handle before describing any, so each is closed even when its description is skipped.
    let handles: Vec<(Handle, LUID)> = infos
        .into_iter()
        .take(request.NumAdapters as usize)
        .map(|info| (Handle(info.hAdapter), info.AdapterLuid))
        .collect();
    Ok(handles
        .iter()
        .filter_map(|(handle, luid)| describe(handle, *luid).ok())
        .collect())
}

/// What one adapter performance query returns that the overlay keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuPerf {
    /// Tenths of a degree Celsius; 0 when the driver reports none.
    pub temperature_deci_c: u32,
    /// 0 when the driver reports none, and for a zero-rpm fan at rest.
    pub fan_rpm: u32,
    /// Memory clock in hertz; 0 when the driver reports none.
    pub memory_hz: u64,
}

/// The performance data of the adapter with this LUID, from one query.
pub fn gpu_perf_data(luid: u64) -> Result<GpuPerf, Win32Error> {
    let mut open = D3DKMT_OPENADAPTERFROMLUID {
        AdapterLuid: LUID {
            LowPart: u32::try_from(luid & 0xFFFF_FFFF).unwrap_or(0),
            HighPart: u32::try_from(luid >> 32).unwrap_or(0).cast_signed(),
        },
        hAdapter: 0,
    };
    // SAFETY: the pointer comes from a live, exclusively borrowed local.
    unsafe { D3DKMTOpenAdapterFromLuid(&raw mut open) }.ok()?;
    let adapter = Handle(open.hAdapter);
    let perf: D3DKMT_ADAPTER_PERFDATA = query(&adapter, KMTQAITYPE_ADAPTERPERFDATA)?;
    Ok(GpuPerf {
        temperature_deci_c: perf.Temperature,
        fan_rpm: perf.FanRPM,
        memory_hz: perf.MemoryFrequency,
    })
}

#[cfg(test)]
mod tests;
