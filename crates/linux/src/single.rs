use std::io::{self, ErrorKind};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{SocketAddr, UnixListener};

const PROCESS_DIRECTORY: &str = "/proc/self";
const ADDRESS_PREFIX: &str = "pith";

pub struct Instance {
    _listener: UnixListener,
}

pub fn single_instance() -> io::Result<Option<Instance>> {
    let owner = std::fs::metadata(PROCESS_DIRECTORY)?.uid();
    let address = SocketAddr::from_abstract_name(format!("{ADDRESS_PREFIX}-{owner}"))?;
    match UnixListener::bind_addr(&address) {
        Ok(listener) => Ok(Some(Instance {
            _listener: listener,
        })),
        Err(error) if error.kind() == ErrorKind::AddrInUse => Ok(None),
        Err(error) => Err(error),
    }
}
