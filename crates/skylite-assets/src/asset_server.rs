use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::AssetError;

const SERVER_SOCKET: &'static str = "socket";
const SERVER_LOCK: &'static str = "lock";

static SERVER_SCRIPT: &'static str = concat!(
    "(module asset-server racket\n",
    ";", /* Used to comment out '#lang racket', as it is not allowed when running the script
         with `racket --load -` */
    include_str!("../asset-server/asset-server.rkt"),
    ") (require (submod 'asset-server main))"
);

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
    use crate::error::AssetError;

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
            std::fs::create_dir_all(&server_tmp_dir)
                .map_err(|err| AssetError::OtherError(err.to_string()))?;
        }

        let socket = server_tmp_dir.join(SERVER_SOCKET);

        if socket.exists() {
            // Socket already exists, try to connect to the asset server
            let stream_res =
                UnixStream::connect(&socket).map_err(|err| AssetError::OtherError(err.to_string()));

            if stream_res.is_ok() {
                return Ok(AssetServerConnection {
                    socket_stream: stream_res.unwrap(),
                });
            }

            // The socket exists, but the asset-server is not running.
            std::fs::remove_file(&socket).map_err(|err| AssetError::OtherError(err.to_string()))?;
        }

        // The asset server is not running, try to launch it.

        let lock_path = server_tmp_dir.join(SERVER_LOCK);
        let lock_file = if !lock_path.exists() {
            File::create(lock_path).map_err(|err| AssetError::OtherError(err.to_string()))?
        } else {
            File::open(lock_path).map_err(|err| AssetError::OtherError(err.to_string()))?
        };

        let lock_file_fd = lock_file.as_raw_fd();

        unsafe { flock(lock_file_fd, LOCK_EX) };

        if socket.exists() {
            // The asset-server was started by another process while
            // we were waiting for the lock.
            unsafe { flock(lock_file_fd, LOCK_UN) };

            return Ok(AssetServerConnection {
                socket_stream: UnixStream::connect(socket)
                    .map_err(|err| AssetError::OtherError(err.to_string()))?,
            });
        }

        start_asset_server(&server_tmp_dir)?;

        unsafe { flock(lock_file_fd, LOCK_UN) };
        Ok(AssetServerConnection {
            socket_stream: UnixStream::connect(socket)
                .map_err(|err| AssetError::OtherError(err.to_string()))?,
        })
    }
}

fn start_asset_server(cwd: &Path) -> Result<(), AssetError> {
    let mut command = Command::new("racket");
    command
        .args(["--load", "-"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_family = "unix")]
    command.process_group(0);
    #[cfg(target_family = "windows")]
    command.creation_flags(0x00000200); // CREATE_NEW_PROCESS_GROUP

    let mut child = command
        .spawn()
        .map_err(|err| AssetError::OtherError(err.to_string()))?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(SERVER_SCRIPT.as_bytes())
        .map_err(|err| AssetError::OtherError(err.to_string()))?;

    // Wait for the asset-server to open its socket.
    let socket = cwd.join(SERVER_SOCKET);
    while !socket
        .try_exists()
        .map_err(|err| AssetError::OtherError(err.to_string()))?
        && child
            .try_wait()
            .map_err(|err| AssetError::OtherError(err.to_string()))?
            .is_none()
    {
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
