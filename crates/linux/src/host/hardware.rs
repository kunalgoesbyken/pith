use pith_core::Hardware;

const MEMINFO: &str = "/proc/meminfo";
const TOTAL_KEY: &str = "MemTotal:";
const KIB_PER_MIB: u64 = 1024;
const FALLBACK_CORES: usize = 1;

fn total_kib() -> Option<u64> {
    std::fs::read_to_string(MEMINFO)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix(TOTAL_KEY))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

pub fn hardware() -> Hardware {
    Hardware {
        ram_mib: total_kib().unwrap_or(0) / KIB_PER_MIB,
        cores: std::thread::available_parallelism().map_or(FALLBACK_CORES, |cores| cores.get()),
    }
}
