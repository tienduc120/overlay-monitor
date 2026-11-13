//! Performance counters with one instance per object (PDH).

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::core::{HRESULT, HSTRING};

use crate::Win32Error;

/// `CStatus` values that carry a usable number: valid data and new data.
const VALID_DATA: u32 = 0;
const NEW_DATA: u32 = 1;
/// The instance list can grow between the sizing call and the filling call, so sizing is retried.
const SIZING_ATTEMPTS: usize = 3;

fn check(status: u32) -> Result<(), Win32Error> {
    if status == ERROR_SUCCESS.0 {
        Ok(())
    } else {
        Err(Win32Error::from_hresult(HRESULT(status.cast_signed())))
    }
}

/// One wildcard counter, read as a value per instance.
#[derive(Debug)]
pub struct CounterArray {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    primed: bool,
}

// SAFETY: the PDH handles are not tied to a thread, and `&mut self` keeps every call to them exclusive.
unsafe impl Send for CounterArray {}

impl CounterArray {
    /// `english_path` is the counter path in English, such as `\GPU Engine(*)\Utilization Percentage`.
    pub fn open(english_path: &str) -> Result<Self, Win32Error> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: a null data source means the live system, and `query` is a live, exclusively borrowed local.
        check(unsafe { PdhOpenQueryW(None, 0, &raw mut query) })?;
        // From here `drop` closes the query on every early return.
        let mut opened = Self {
            query,
            counter: PDH_HCOUNTER::default(),
            primed: false,
        };
        let path = HSTRING::from(english_path);
        // SAFETY: `path` is a live null-terminated UTF-16 string, and the counter handle is written to a live local.
        check(unsafe { PdhAddEnglishCounterW(opened.query, &path, 0, &raw mut opened.counter) })?;
        Ok(opened)
    }

    /// One value per instance. The first call only primes the rate and returns nothing.
    pub fn collect(&mut self) -> Result<Vec<(String, f64)>, Win32Error> {
        // SAFETY: the query handle is open until `drop`.
        check(unsafe { PdhCollectQueryData(self.query) })?;
        if !std::mem::replace(&mut self.primed, true) {
            return Ok(Vec::new());
        }
        self.formatted()
    }

    fn formatted(&self) -> Result<Vec<(String, f64)>, Win32Error> {
        let mut bytes = 0u32;
        let mut count = 0u32;
        for _ in 0..SIZING_ATTEMPTS {
            // SAFETY: with no buffer the call only reports the needed size and count into live locals.
            let status = unsafe {
                PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &raw mut bytes, &raw mut count, None)
            };
            if status != PDH_MORE_DATA {
                check(status)?;
                return Ok(Vec::new());
            }
            // `u64` elements keep the buffer aligned for the items it holds.
            let mut buffer = vec![0u64; (bytes as usize).div_ceil(size_of::<u64>())];
            let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            // SAFETY: `items` points at `bytes` writable, 8-aligned bytes owned by `buffer`.
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &raw mut bytes,
                    &raw mut count,
                    Some(items),
                )
            };
            if status == PDH_MORE_DATA {
                continue;
            }
            check(status)?;
            // SAFETY: on success the buffer holds `count` items whose names point inside the same buffer.
            return Ok(unsafe { read_items(items, count as usize) });
        }
        Err(Win32Error::from_hresult(HRESULT(PDH_MORE_DATA.cast_signed())))
    }
}

/// # Safety
/// `items` must point at `count` initialised items, and their name pointers must be valid null-terminated UTF-16.
unsafe fn read_items(items: *const PDH_FMT_COUNTERVALUE_ITEM_W, count: usize) -> Vec<(String, f64)> {
    // SAFETY: the caller guarantees `count` initialised items at `items`.
    let items = unsafe { std::slice::from_raw_parts(items, count) };
    items
        .iter()
        .filter(|item| matches!(item.FmtValue.CStatus, VALID_DATA | NEW_DATA))
        .map(|item| {
            // SAFETY: the caller guarantees a valid null-terminated name; the value was requested as a double.
            unsafe {
                (
                    String::from_utf16_lossy(item.szName.as_wide()),
                    item.FmtValue.Anonymous.doubleValue,
                )
            }
        })
        .collect()
}

impl Drop for CounterArray {
    fn drop(&mut self) {
        // SAFETY: the query was opened in `open` and is closed exactly once, here; closing also frees its counter.
        // A failed close leaves nothing to recover, so its status is ignored.
        let _ = unsafe { PdhCloseQuery(self.query) };
    }
}

#[cfg(test)]
mod tests;
