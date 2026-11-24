// GPU load without nvidia-smi (AMD, Intel): the same Windows counters Task Manager uses.
use std::sync::{Mutex, OnceLock};
use windows::core::{w, PCWSTR};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Performance::*;

pub struct Gpu { pub name: String, pub load: f64, pub used: u64, pub total: u64 }

/// the graphics card with the most own memory: (name, bytes)
fn card() -> Option<&'static (String, u64)> {
    static CARD: OnceLock<Option<(String, u64)>> = OnceLock::new();
    CARD.get_or_init(|| unsafe {
        let f: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: Option<(String, u64)> = None;
        for i in 0.. {
            let Ok(a) = f.EnumAdapters1(i) else { break };
            let Ok(d) = a.GetDesc1() else { continue };
            if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 { continue; }
            let end = d.Description.iter().position(|c| *c == 0).unwrap_or(128);
            let name = String::from_utf16_lossy(&d.Description[..end]);
            if best.as_ref().map_or(true, |b| d.DedicatedVideoMemory as u64 > b.1) { best = Some((name, d.DedicatedVideoMemory as u64)); }
        }
        best
    }).as_ref()
}

struct Counters { query: PDH_HQUERY, load: PDH_HCOUNTER, mem: PDH_HCOUNTER }
unsafe impl Send for Counters {}
static COUNTERS: Mutex<Option<Counters>> = Mutex::new(None);

unsafe fn values(c: PDH_HCOUNTER) -> Vec<f64> {
    let (mut size, mut n) = (0u32, 0u32);
    PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut n, None);
    if size == 0 { return vec![]; }
    let item = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::with_capacity(size as usize / item + 1);
    if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut n, Some(buf.as_mut_ptr())) != 0 { return vec![]; }
    buf.set_len(n as usize);
    buf.iter().map(|i| i.FmtValue.Anonymous.doubleValue).filter(|v| v.is_finite()).collect()
}

pub fn read() -> Option<Gpu> {
    let (name, total) = card()?;
    let mut g = COUNTERS.lock().ok()?;
    unsafe {
        if g.is_none() {
            let mut c = Counters { query: Default::default(), load: Default::default(), mem: Default::default() };
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut c.query) != 0 { return None; }
            PdhAddEnglishCounterW(c.query, w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"), 0, &mut c.load);
            PdhAddEnglishCounterW(c.query, w!("\\GPU Adapter Memory(*)\\Dedicated Usage"), 0, &mut c.mem);
            PdhCollectQueryData(c.query); // the load needs two samples
            *g = Some(c);
        }
        let c = g.as_ref()?;
        PdhCollectQueryData(c.query);
        Some(Gpu {
            name: name.clone(), total: *total,
            load: values(c.load).iter().sum::<f64>().clamp(0.0, 100.0),
            used: values(c.mem).iter().cloned().fold(0.0, f64::max) as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads() {
        super::read();
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let g = super::read().expect("no gpu");
        println!("{} load {:.1} used {} total {}", g.name, g.load, g.used, g.total);
        assert!(!g.name.is_empty() && g.used > 0);
    }
}
