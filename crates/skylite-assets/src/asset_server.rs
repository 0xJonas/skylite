use std::io::Write;
#[cfg(target_family = "unix")]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::assets::{AssetError, AssetType};
use crate::base_serde::Serialize;
use crate::path_to_native;

const SERVER_SOCKET: &'static str = "socket";
const SERVER_LOCK: &'static str = "lock";

static SERVER_MODULES: [(&'static str, &'static str); 5] = [
    (
        "log-trace.rkt",
        include_str!("../asset-server/log-trace.rkt"),
    ),
    ("project.rkt", include_str!("../asset-server/project.rkt")),
    ("serde.rkt", include_str!("../asset-server/serde.rkt")),
    ("validate.rkt", include_str!("../asset-server/validate.rkt")),
    (
        "asset-server.rkt",
        include_str!("../asset-server/asset-server.rkt"),
    ),
];

#[cfg(target_family = "unix")]
mod unix {
    use std::env::temp_dir;
    use std::ffi::c_int;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    use super::start_asset_server;
    use crate::asset_server::{SERVER_LOCK, SERVER_SOCKET};
    use crate::assets::AssetError;

    const LOCK_EX: c_int = 2;
    const LOCK_UN: c_int = 8;

    unsafe extern "C" {
        unsafe fn flock(fd: c_int, operation: c_int) -> c_int;
    }

    pub struct AssetServerConnection {
        socket_stream: UnixStream,
    }

    impl Read for AssetServerConnection {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.socket_stream.read(buf)
        }
    }

    impl Write for AssetServerConnection {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.socket_stream.write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.socket_stream.flush()
        }
    }

    impl Drop for AssetServerConnection {
        fn drop(&mut self) {
            let _ = self.socket_stream.shutdown(std::net::Shutdown::Both);
        }
    }

    pub(crate) fn connect_to_asset_server() -> Result<AssetServerConnection, AssetError> {
        let server_tmp_dir = temp_dir().join("skylite").join("asset-server");
        if !server_tmp_dir.is_dir() {
            std::fs::create_dir_all(&server_tmp_dir)?;
        }

        let socket = server_tmp_dir.join(SERVER_SOCKET);

        if socket.exists() {
            // Socket already exists, try to connect to the asset server
            let stream_res = UnixStream::connect(&socket);

            if stream_res.is_ok() {
                return Ok(AssetServerConnection {
                    socket_stream: stream_res.unwrap(),
                });
            }

            // The socket exists, but the asset-server is not running.
            std::fs::remove_file(&socket)?;
        }

        // The asset server is not running, try to launch it.

        let lock_path = server_tmp_dir.join(SERVER_LOCK);
        let lock_file = if !lock_path.exists() {
            File::create(lock_path)?
        } else {
            File::open(lock_path)?
        };

        let lock_file_fd = lock_file.as_raw_fd();

        unsafe { flock(lock_file_fd, LOCK_EX) };

        if socket.exists() {
            // The asset-server was started by another process while
            // we were waiting for the lock.
            unsafe { flock(lock_file_fd, LOCK_UN) };

            return Ok(AssetServerConnection {
                socket_stream: UnixStream::connect(socket)?,
            });
        }

        start_asset_server(&server_tmp_dir)?;

        unsafe { flock(lock_file_fd, LOCK_UN) };
        Ok(AssetServerConnection {
            socket_stream: UnixStream::connect(socket)?,
        })
    }
}

fn start_asset_server(cwd: &Path) -> Result<(), AssetError> {
    for (filename, content) in SERVER_MODULES {
        std::fs::write(cwd.join(filename), content.as_bytes())?;
    }

    let mut command = Command::new("racket");
    command
        .args(["--require", "asset-server.rkt"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_family = "unix")]
    command.process_group(0);
    #[cfg(target_family = "windows")]
    command.creation_flags(0x00000200); // CREATE_NEW_PROCESS_GROUP

    let mut child = command.spawn()?;

    // Wait for the asset-server to open its socket.
    let socket = cwd.join(SERVER_SOCKET);
    while !socket.try_exists()? && child.try_wait()?.is_none() {
        std::thread::yield_now();
    }

    // child will be dropped here, which automatically closes our ends
    // of the piped stdio streams.
    Ok(())
}

#[cfg(target_family = "unix")]
pub(crate) use unix::{connect_to_asset_server, AssetServerConnection};

#[cfg(not(target_family = "unix"))]
compile_error!("This platform is currently not supported.");

const REQ_TYPE_RETRIEVE_ASSET: u8 = 0;
const REQ_TYPE_LIST_ASSETS: u8 = 1;
const REQ_TYPE_CLEAR_CACHE: u8 = 2;
const REQ_TYPE_SHUTDOWN: u8 = 3;

impl AssetServerConnection {
    pub(crate) fn send_load_asset_request(
        &mut self,
        project_path: &Path,
        atype: AssetType,
        name: &str,
    ) -> Result<(), AssetError> {
        REQ_TYPE_RETRIEVE_ASSET.serialize(self)?;
        path_to_native(project_path).as_slice().serialize(self)?;

        match atype {
            AssetType::Project => 0u8.serialize(self)?,
            AssetType::Node => 1u8.serialize(self)?,
            AssetType::NodeList => 2u8.serialize(self)?,
            AssetType::Sequence => 3u8.serialize(self)?,
        }
        name.serialize(self)?;
        self.flush()?;
        Ok(())
    }
}
