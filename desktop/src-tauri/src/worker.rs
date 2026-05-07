// SPDX-License-Identifier: AGPL-3.0-only
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde_json::{json, Value};

use crate::errors::{HostError, HostResult};
use crate::models::WorkerJobResult;
use crate::protocol::{hello_request, read_frame, write_frame, ProtocolMessage, ProtocolRequest};
use crate::worker_runtime::{resolve_worker_launch_spec, WorkerRuntimeSource};

#[cfg(not(windows))]
use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::fs::File;
#[cfg(windows)]
use std::mem;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
#[cfg(windows)]
use std::ptr;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
#[cfg(windows)]
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
#[cfg(windows)]
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

const WORKER_MARKER_PREFIX: &str = "scriptscore-desktop-worker-";
#[cfg(windows)]
const WINDOWS_PIPE_WRITE_CHUNK_BYTES: usize = 16 * 1024;

fn worker_marker_path(worker_pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("{WORKER_MARKER_PREFIX}{worker_pid}.pid"))
}

fn write_worker_marker(worker_pid: u32) -> Option<PathBuf> {
    let marker_path = worker_marker_path(worker_pid);
    let contents = format!("worker_pid={worker_pid}\nhost_pid={}\n", std::process::id());
    std::fs::write(&marker_path, contents)
        .map(|()| marker_path)
        .ok()
}

fn cleanup_orphaned_worker_markers() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(WORKER_MARKER_PREFIX) || !name.ends_with(".pid") {
            continue;
        }
        cleanup_orphaned_worker_marker(&path);
    }
}

#[cfg(not(windows))]
fn cleanup_orphaned_worker_marker(path: &Path) {
    let Some((worker_pid, host_pid)) = read_worker_marker(path) else {
        let _ = std::fs::remove_file(path);
        return;
    };
    if host_pid == std::process::id() || process_exists(host_pid) {
        return;
    }
    if is_scriptscore_worker_process(worker_pid) {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(worker_pid.to_string())
            .status();
    }
    let _ = std::fs::remove_file(path);
}

#[cfg(windows)]
fn cleanup_orphaned_worker_marker(path: &Path) {
    let Some((_, host_pid)) = read_worker_marker(path) else {
        let _ = std::fs::remove_file(path);
        return;
    };
    if host_pid != std::process::id() {
        let _ = std::fs::remove_file(path);
    }
}

fn read_worker_marker(path: &Path) -> Option<(u32, u32)> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut worker_pid = None;
    let mut host_pid = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("worker_pid=") {
            worker_pid = value.trim().parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("host_pid=") {
            host_pid = value.trim().parse::<u32>().ok();
        }
    }
    Some((worker_pid?, host_pid?))
}

#[cfg(not(windows))]
fn process_exists(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(windows))]
fn is_scriptscore_worker_process(pid: u32) -> bool {
    let cmdline_path = Path::new("/proc").join(pid.to_string()).join("cmdline");
    if let Ok(cmdline) = std::fs::read(cmdline_path) {
        return cmdline
            .split(|byte| *byte == 0)
            .filter_map(|part| std::str::from_utf8(part).ok())
            .any(|part| part == "scriptscore.transport.desktop_worker");
    }
    Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("command=")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|command| command.contains("scriptscore.transport.desktop_worker"))
}

pub struct CompletedWorkerJob {
    pub job_id: String,
    pub result: WorkerJobResult,
}

pub struct WorkerCancelHandle {
    stream: WorkerStream,
    next_request_id: Arc<AtomicU64>,
}

impl WorkerCancelHandle {
    pub fn cancel(&mut self, job_id: &str) -> HostResult<()> {
        let request = ProtocolRequest {
            request_type: "cancel_job",
            request_id: next_request_id(&self.next_request_id),
            job_id: Some(job_id.to_string()),
            payload: json!({}),
        };
        write_frame(&mut self.stream, &request)
    }
}

pub struct WorkerClient {
    child: Child,
    stream: WorkerStream,
    marker_path: Option<PathBuf>,
    #[cfg(not(windows))]
    socket_path: PathBuf,
    next_request_id: Arc<AtomicU64>,
}

impl WorkerClient {
    pub fn launch(
        bundled_resource_dir: Option<&Path>,
        runtime_source: WorkerRuntimeSource,
    ) -> HostResult<Self> {
        cleanup_orphaned_worker_markers();
        let launch_spec = resolve_worker_launch_spec(bundled_resource_dir, runtime_source)?;

        #[cfg(windows)]
        {
            let pipe_name = pipe_name();
            let request_pipe_name = format!("{pipe_name}-request");
            let response_pipe_name = format!("{pipe_name}-response");
            let request_handle = create_named_pipe_server(&request_pipe_name)?;
            let response_handle = create_named_pipe_server(&response_pipe_name)?;
            let mut command = Command::new(&launch_spec.python_executable);
            command
                .current_dir(&launch_spec.current_dir)
                .arg("-m")
                .arg("scriptscore.transport.desktop_worker")
                .arg("--pipe-name")
                .arg(&pipe_name)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Some(python_path) = &launch_spec.python_path {
                command.env("PYTHONPATH", python_path);
            }
            for (key, value) in &launch_spec.extra_env {
                command.env(key, value);
            }
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(err) => {
                    close_pipe_handle(request_handle);
                    close_pipe_handle(response_handle);
                    return Err(err.into());
                }
            };
            let writer = match accept_worker_named_pipe(request_handle, &mut child) {
                Ok(pipe) => pipe,
                Err(err) => {
                    close_pipe_handle(response_handle);
                    return Err(err);
                }
            };
            let reader = accept_worker_named_pipe(response_handle, &mut child)?;
            let stream = WorkerStream::Pipe(WindowsPipeStream::new(reader, writer));
            let marker_path = write_worker_marker(child.id());
            let mut client = Self {
                child,
                stream,
                marker_path,
                next_request_id: Arc::new(AtomicU64::new(1)),
            };
            client.hello()?;
            Ok(client)
        }

        #[cfg(not(windows))]
        {
            let socket_path = socket_path()?;
            let _ = std::fs::remove_file(&socket_path);
            let listener = UnixListener::bind(&socket_path)?;
            listener.set_nonblocking(true)?;
            let mut command = Command::new(&launch_spec.python_executable);
            command
                .current_dir(&launch_spec.current_dir)
                .arg("-m")
                .arg("scriptscore.transport.desktop_worker")
                .arg("--socket-path")
                .arg(&socket_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Some(python_path) = &launch_spec.python_path {
                command.env("PYTHONPATH", python_path);
            }
            for (key, value) in &launch_spec.extra_env {
                command.env(key, value);
            }
            let mut child = command.spawn()?;

            let stream = WorkerStream::Unix(accept_worker_connection(&listener, &mut child)?);
            let marker_path = write_worker_marker(child.id());
            let mut client = Self {
                child,
                stream,
                marker_path,
                socket_path,
                next_request_id: Arc::new(AtomicU64::new(1)),
            };
            client.hello()?;
            Ok(client)
        }
    }

    pub fn cancel_handle(&self) -> HostResult<WorkerCancelHandle> {
        Ok(WorkerCancelHandle {
            stream: self.stream.try_clone()?,
            next_request_id: Arc::clone(&self.next_request_id),
        })
    }

    pub fn reserve_job_ids(&mut self) -> (String, String) {
        (self.next_request_id(), self.next_job_id())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_job<F>(
        &mut self,
        request_id: String,
        job_id: String,
        command_name: &str,
        request_payload: Value,
        output_artifacts_dir: Option<&Path>,
        stdin_bytes: Option<&[u8]>,
        mut on_message: F,
    ) -> HostResult<CompletedWorkerJob>
    where
        F: FnMut(&ProtocolMessage) -> HostResult<()>,
    {
        let stdin_b64 =
            stdin_bytes.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
        let payload = json!({
            "command_name": command_name,
            "request": request_payload,
            "output_artifacts_dir": output_artifacts_dir.map(|path| path.to_string_lossy().into_owned()),
            "stdin_b64": stdin_b64,
        });
        let request = ProtocolRequest {
            request_type: "run_job",
            request_id: request_id.clone(),
            job_id: Some(job_id.clone()),
            payload,
        };
        write_frame(&mut self.stream, &request)?;
        self.await_terminal(request_id, job_id, &mut on_message)
    }

    pub fn shutdown(&mut self) {
        let request = ProtocolRequest {
            request_type: "shutdown",
            request_id: self.next_request_id(),
            job_id: None,
            payload: json!({}),
        };
        let _ = write_frame(&mut self.stream, &request);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                Err(_) => break,
            }
        }
        if let Some(marker_path) = self.marker_path.take() {
            let _ = std::fs::remove_file(marker_path);
        }
        #[cfg(not(windows))]
        let _ = std::fs::remove_file(&self.socket_path);
    }

    fn await_terminal(
        &mut self,
        request_id: String,
        job_id: String,
        on_message: &mut dyn FnMut(&ProtocolMessage) -> HostResult<()>,
    ) -> HostResult<CompletedWorkerJob> {
        let mut events = Vec::new();
        loop {
            let message = read_frame(&mut self.stream)?.ok_or_else(|| {
                HostError::Worker("Desktop worker disconnected before a terminal response.".into())
            })?;
            if message.request_id.as_deref() != Some(request_id.as_str()) {
                continue;
            }
            if matches!(
                message.message_type.as_str(),
                "job_started" | "job_progress" | "job_finished" | "job_failed" | "job_cancelled"
            ) && message.job_id.as_deref() != Some(job_id.as_str())
            {
                continue;
            }
            match message.message_type.as_str() {
                "job_started" => {
                    on_message(&message)?;
                }
                "job_progress" => {
                    events.push(message.payload.clone());
                    on_message(&message)?;
                }
                "job_finished" | "job_failed" | "job_cancelled" => {
                    let _exit_code = message
                        .payload
                        .get("exit_code")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| {
                            HostError::Protocol(
                                "Worker terminal message was missing exit_code.".into(),
                            )
                        })?;
                    let envelope = message.payload.get("envelope").cloned().ok_or_else(|| {
                        HostError::Protocol("Worker terminal message was missing envelope.".into())
                    })?;
                    return Ok(CompletedWorkerJob {
                        job_id,
                        result: WorkerJobResult {
                            terminal_type: message.message_type,
                            terminal_payload: message.payload.clone(),
                            envelope,
                            events,
                        },
                    });
                }
                "error" => return Err(worker_error(&message)),
                _ => continue,
            }
        }
    }

    fn hello(&mut self) -> HostResult<()> {
        let request_id = self.next_request_id();
        write_frame(&mut self.stream, &hello_request(request_id.clone()))?;
        let response = read_frame(&mut self.stream)?.ok_or_else(|| {
            HostError::Worker("Desktop worker disconnected before hello_ok.".into())
        })?;
        match response.message_type.as_str() {
            "hello_ok" if response.request_id.as_deref() == Some(request_id.as_str()) => Ok(()),
            "error" => Err(worker_error(&response)),
            other => Err(HostError::Protocol(format!(
                "Unexpected worker hello response '{other}'."
            ))),
        }
    }

    fn next_job_id(&mut self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis();
        let uuid = uuid::Uuid::new_v4();
        format!("job_{:x}_{}", timestamp, uuid)
    }

    fn next_request_id(&self) -> String {
        next_request_id(&self.next_request_id)
    }
}

impl Drop for WorkerClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum WorkerStream {
    #[cfg(not(windows))]
    Unix(UnixStream),
    #[cfg(windows)]
    Pipe(WindowsPipeStream),
}

impl WorkerStream {
    fn try_clone(&self) -> HostResult<Self> {
        match self {
            #[cfg(not(windows))]
            Self::Unix(stream) => Ok(Self::Unix(stream.try_clone()?)),
            #[cfg(windows)]
            Self::Pipe(pipe) => Ok(Self::Pipe(pipe.try_clone()?)),
        }
    }
}

impl Read for WorkerStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(not(windows))]
            Self::Unix(stream) => stream.read(buf),
            #[cfg(windows)]
            Self::Pipe(pipe) => pipe.read(buf),
        }
    }
}

impl Write for WorkerStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(not(windows))]
            Self::Unix(stream) => stream.write(buf),
            #[cfg(windows)]
            Self::Pipe(pipe) => pipe.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            #[cfg(not(windows))]
            Self::Unix(stream) => stream.flush(),
            #[cfg(windows)]
            Self::Pipe(pipe) => pipe.flush(),
        }
    }
}

#[cfg(windows)]
struct WindowsPipeStream {
    reader: File,
    writer: File,
}

#[cfg(windows)]
impl WindowsPipeStream {
    fn new(reader: File, writer: File) -> Self {
        Self { reader, writer }
    }

    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(Self::new(
            self.reader.try_clone()?,
            self.writer.try_clone()?,
        ))
    }

    fn read_handle(&self) -> HANDLE {
        self.reader.as_raw_handle() as HANDLE
    }

    fn write_handle(&self) -> HANDLE {
        self.writer.as_raw_handle() as HANDLE
    }
}

#[cfg(windows)]
impl Read for WindowsPipeStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let event_handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event_handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let _event = unsafe { OwnedHandle::from_raw_handle(event_handle as RawHandle) };
        let mut overlapped: OVERLAPPED = unsafe { mem::zeroed() };
        overlapped.hEvent = event_handle;

        let read = unsafe {
            ReadFile(
                self.read_handle(),
                buf.as_mut_ptr().cast(),
                buf.len().try_into().unwrap_or(u32::MAX),
                ptr::null_mut(),
                &mut overlapped,
            )
        };
        wait_for_overlapped_pipe_io(self.read_handle(), &mut overlapped, read)
    }
}

#[cfg(windows)]
impl Write for WindowsPipeStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let chunk_len = buf.len().min(WINDOWS_PIPE_WRITE_CHUNK_BYTES);
        let chunk = &buf[..chunk_len];
        let event_handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event_handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let _event = unsafe { OwnedHandle::from_raw_handle(event_handle as RawHandle) };
        let mut overlapped: OVERLAPPED = unsafe { mem::zeroed() };
        overlapped.hEvent = event_handle;

        let written = unsafe {
            WriteFile(
                self.write_handle(),
                chunk.as_ptr().cast(),
                chunk.len().try_into().unwrap_or(u32::MAX),
                ptr::null_mut(),
                &mut overlapped,
            )
        };
        wait_for_overlapped_pipe_io(self.write_handle(), &mut overlapped, written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
fn wait_for_overlapped_pipe_io(
    handle: HANDLE,
    overlapped: &mut OVERLAPPED,
    immediate_result: i32,
) -> std::io::Result<usize> {
    if immediate_result == 0 {
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code) if code == ERROR_IO_PENDING as i32 => {}
            Some(code) if code == ERROR_BROKEN_PIPE as i32 => return Ok(0),
            _ => return Err(error),
        }
    }

    let mut transferred = 0;
    let completed = unsafe { GetOverlappedResult(handle, overlapped, &mut transferred, 1) };
    if completed == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
            return Ok(0);
        }
        return Err(error);
    }
    Ok(transferred as usize)
}

#[cfg(not(windows))]
fn accept_worker_connection(listener: &UnixListener, child: &mut Child) -> HostResult<UnixStream> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => return Ok(stream),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if child.try_wait()?.is_some() {
                    return Err(HostError::Worker(
                        "Desktop worker exited before connecting to the socket.".into(),
                    ));
                }
                if Instant::now() >= deadline {
                    return Err(HostError::Worker(
                        "Timed out waiting for the desktop worker to connect.".into(),
                    ));
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(err.into()),
        }
    }
}

#[cfg(not(windows))]
fn socket_path() -> HostResult<PathBuf> {
    let mut path = std::env::temp_dir();
    path.push(format!("scriptscore-desktop-{}.sock", std::process::id()));
    Ok(path)
}

#[cfg(windows)]
fn pipe_name() -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis();
    format!("scriptscore-desktop-{}-{nonce}", std::process::id())
}

#[cfg(windows)]
fn accept_worker_named_pipe(handle: HANDLE, child: &mut Child) -> HostResult<File> {
    let connect_state = begin_named_pipe_connect(handle)?;
    if connect_state.connected {
        return Ok(file_from_pipe_handle(handle));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match wait_for_named_pipe_connect(handle, connect_state.overlapped.as_ref(), 20)? {
            PipeConnectPoll::Connected => return Ok(file_from_pipe_handle(handle)),
            PipeConnectPoll::Pending => {
                if child.try_wait()?.is_some() {
                    cancel_named_pipe_connect(handle, connect_state.overlapped.as_ref());
                    close_pipe_handle(handle);
                    return Err(HostError::Worker(
                        "Desktop worker exited before connecting to the named pipe.".into(),
                    ));
                }
                if Instant::now() >= deadline {
                    cancel_named_pipe_connect(handle, connect_state.overlapped.as_ref());
                    close_pipe_handle(handle);
                    return Err(HostError::Worker(
                        "Timed out waiting for the desktop worker to connect.".into(),
                    ));
                }
            }
        }
    }
}

#[cfg(windows)]
struct PendingNamedPipeConnect {
    connected: bool,
    overlapped: Box<OVERLAPPED>,
    _event: OwnedHandle,
}

#[cfg(windows)]
enum PipeConnectPoll {
    Pending,
    Connected,
}

#[cfg(windows)]
fn begin_named_pipe_connect(handle: HANDLE) -> HostResult<PendingNamedPipeConnect> {
    let event_handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
    if event_handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    let event = unsafe { OwnedHandle::from_raw_handle(event_handle as RawHandle) };
    let mut overlapped: Box<OVERLAPPED> = Box::new(unsafe { mem::zeroed() });
    overlapped.hEvent = event_handle;

    let connected = unsafe { ConnectNamedPipe(handle, overlapped.as_mut()) };
    if connected != 0 {
        return Ok(PendingNamedPipeConnect {
            connected: true,
            overlapped,
            _event: event,
        });
    }

    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(code) if code == ERROR_IO_PENDING as i32 => Ok(PendingNamedPipeConnect {
            connected: false,
            overlapped,
            _event: event,
        }),
        Some(code) if code == ERROR_PIPE_CONNECTED as i32 => Ok(PendingNamedPipeConnect {
            connected: true,
            overlapped,
            _event: event,
        }),
        _ => Err(error.into()),
    }
}

#[cfg(windows)]
fn wait_for_named_pipe_connect(
    handle: HANDLE,
    overlapped: &OVERLAPPED,
    wait_ms: u32,
) -> HostResult<PipeConnectPoll> {
    let wait_result = unsafe { WaitForSingleObject(overlapped.hEvent, wait_ms) };
    match wait_result {
        WAIT_OBJECT_0 => {
            let mut transferred = 0;
            let connected = unsafe { GetOverlappedResult(handle, overlapped, &mut transferred, 0) };
            if connected == 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
                    return Ok(PipeConnectPoll::Connected);
                }
                close_pipe_handle(handle);
                return Err(error.into());
            }
            Ok(PipeConnectPoll::Connected)
        }
        WAIT_TIMEOUT => Ok(PipeConnectPoll::Pending),
        _ => Err(std::io::Error::last_os_error().into()),
    }
}

#[cfg(windows)]
fn create_named_pipe_server(pipe_name: &str) -> HostResult<HANDLE> {
    let pipe_path = format!(r"\\.\pipe\{pipe_name}");
    let wide_name: Vec<u16> = OsStr::new(&pipe_path)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let handle = unsafe {
        CreateNamedPipeW(
            wide_name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            64 * 1024,
            64 * 1024,
            0,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(handle)
}

#[cfg(windows)]
fn cancel_named_pipe_connect(handle: HANDLE, overlapped: &OVERLAPPED) {
    unsafe {
        let _ = CancelIoEx(handle, overlapped);
    }
}

#[cfg(windows)]
fn file_from_pipe_handle(handle: HANDLE) -> File {
    let owned = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
    File::from(owned)
}

#[cfg(windows)]
fn close_pipe_handle(handle: HANDLE) {
    unsafe {
        drop(OwnedHandle::from_raw_handle(handle as RawHandle));
    }
}

fn next_request_id(counter: &Arc<AtomicU64>) -> String {
    format!("req_{:06}", counter.fetch_add(1, Ordering::SeqCst))
}

fn worker_error(message: &ProtocolMessage) -> HostError {
    let code = message
        .payload
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("worker_error");
    let detail = message
        .payload
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Desktop worker returned an error.");
    HostError::Worker(format!("{code}: {detail}"))
}
