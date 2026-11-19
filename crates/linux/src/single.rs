use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{SocketAddr, UnixListener};

const PROCESS_DIRECTORY: &str = "/proc/self";
const ADDRESS_PREFIX: &str = "pith";

pub struct Instance {
    _listener: UnixListener,
}

pub fn single_instance() -> Option<Instance> {
    let owner = std::fs::metadata(PROCESS_DIRECTORY).ok()?.uid();
    let address = SocketAddr::from_abstract_name(format!("{ADDRESS_PREFIX}-{owner}")).ok()?;
    let listener = UnixListener::bind_addr(&address).ok()?;
    Some(Instance {
        _listener: listener,
    })
}
