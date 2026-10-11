//! Small native Win32 launcher. The capture host runs as a managed child process.
#[path = "credentials.rs"]
mod credentials;
#[path = "desktop_style.rs"]
mod desktop_style;
use std::{
    cell::RefCell,
    io::{BufRead, BufReader},
    mem::size_of,
    net::{SocketAddr, UdpSocket},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    ptr::{null, null_mut},
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{DataExchange::*, JobObjects::*, LibraryLoader::*, Memory::*, Threading::*},
    UI::{Controls::*, Input::KeyboardAndMouse::EnableWindow, Shell::*, WindowsAndMessaging::*},
};

const START: usize = 101;
const STOP: usize = 102;
const COPY: usize = 103;
const OPEN: usize = 104;
const HELP: usize = 105;
const EXIT: usize = 106;
const SHOW: usize = 107;
const REMEMBER: usize = 108;
const FORGET: usize = 109;
const DIAGNOSTICS: usize = 110;
const BIND: usize = 201;
const KEY: usize = 202;
const SECRET: usize = 203;
const LINK: usize = 204;
const LOG: usize = 205;
const AUDIO: usize = 206;
const REMOTE: usize = 207;
const TRAY: u32 = WM_APP + 1;
const READY: u32 = WM_APP + 2;
const ICON_CONTROL: u32 = 3; // Win32 SS_ICON static-control style.

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

const INSTANCE_MUTEX: &str = "Local\\MonochromeCapture.Desktop.v1";
const WINDOW_CLASS: &str = "MonochromeDesktopWindow";

/// An owned kernel mutex is released even after an abnormal process exit.
/// Unlike a PID file, it cannot leave a stale lock behind.
struct InstanceGuard(HANDLE);

impl InstanceGuard {
    fn acquire(name: &str) -> std::io::Result<Option<Self>> {
        unsafe {
            let handle = CreateMutexW(null(), 0, wide(name).as_ptr());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            match WaitForSingleObject(handle, 0) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Some(Self(handle))),
                WAIT_TIMEOUT => {
                    CloseHandle(handle);
                    Ok(None)
                }
                _ => {
                    let error = std::io::Error::last_os_error();
                    CloseHandle(handle);
                    Err(error)
                }
            }
        }
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

fn activate_existing_instance(wait: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(if wait { 5 } else { 0 });
    loop {
        unsafe {
            // FindWindow includes windows hidden in the system tray.
            let hwnd = FindWindowW(wide(WINDOW_CLASS).as_ptr(), null());
            if !hwnd.is_null() {
                let mut process_id = 0;
                GetWindowThreadProcessId(hwnd, &mut process_id);
                AllowSetForegroundWindow(process_id);
                // Use the existing Show command; never start or stop its stream.
                PostMessageW(hwnd, WM_COMMAND, SHOW, 0);
                return true;
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
enum Event {
    Host(u64, String),
    Tunnel(u64, String),
}
struct App {
    hwnd: HWND,
    bind: HWND,
    remote: HWND,
    audio: HWND,
    key: HWND,
    secret: HWND,
    remember: HWND,
    forget: HWND,
    status: HWND,
    link: HWND,
    log: HWND,
    start: HWND,
    stop: HWND,
    host: Option<Child>,
    tunnel: Option<Child>,
    job: HANDLE,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    token_fragment: String,
    public_url: String,
    stopping: Option<Instant>,
    lines: String,
    tray_added: bool,
    taskbar_created: u32,
    style: Rc<desktop_style::Style>,
    diagnostics: HWND,
    expanded: bool,
    generation: u64,
}

struct WindowState {
    app: RefCell<App>,
    style: Rc<desktop_style::Style>,
}

impl App {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            hwnd: null_mut(),
            bind: null_mut(),
            remote: null_mut(),
            audio: null_mut(),
            key: null_mut(),
            secret: null_mut(),
            remember: null_mut(),
            forget: null_mut(),
            status: null_mut(),
            link: null_mut(),
            log: null_mut(),
            start: null_mut(),
            stop: null_mut(),
            host: None,
            tunnel: None,
            job: null_mut(),
            tx,
            rx,
            token_fragment: String::new(),
            public_url: String::new(),
            stopping: None,
            lines: String::new(),
            tray_added: false,
            taskbar_created: 0,
            style: Rc::new(desktop_style::Style::new()),
            diagnostics: null_mut(),
            expanded: false,
            generation: 0,
        }
    }
    fn initialize(&mut self) {
        unsafe {
            self.style.title_bar(self.hwnd);
            let logo = self.control(
                "STATIC",
                "Monochrome - Share logo",
                32,
                20,
                64,
                64,
                208,
                ICON_CONTROL,
            );
            let icon = LoadImageW(
                GetModuleHandleW(null()),
                1usize as _,
                IMAGE_ICON,
                self.style.px(64),
                self.style.px(64),
                LR_SHARED,
            );
            SendMessageW(logo, STM_SETICON, icon as usize, 0);
            SendMessageW(
                self.hwnd,
                WM_SETICON,
                ICON_SMALL as usize,
                LoadImageW(
                    GetModuleHandleW(null()),
                    1usize as _,
                    IMAGE_ICON,
                    GetSystemMetrics(SM_CXSMICON),
                    GetSystemMetrics(SM_CYSMICON),
                    LR_SHARED,
                ) as isize,
            );
            self.label("Monochrome - Share", 112, 20, 580, 38, self.style.title);
            self.label(
                "1-to-1 Private screen sharing",
                112,
                62,
                560,
                22,
                self.style.body,
            );
            self.label("Sharing", 48, 120, 292, 28, self.style.heading);
            self.label("LAN address and port", 48, 162, 292, 20, self.style.small);
            let lan = UdpSocket::bind("0.0.0.0:0")
                .and_then(|s| {
                    s.connect("192.0.2.1:80")?;
                    s.local_addr()
                })
                .map(|a| format!("{}:8000", a.ip()))
                .unwrap_or_else(|_| "127.0.0.1:8000".into());
            self.bind = self.control("EDIT", &lan, 58, 196, 280, 22, BIND, ES_AUTOHSCROLL as u32);
            self.audio = self.control(
                "BUTTON",
                "Include desktop audio",
                48,
                246,
                292,
                24,
                AUDIO,
                BS_AUTOCHECKBOX as u32,
            );
            SendMessageW(self.audio, BM_SETCHECK, BST_CHECKED as usize, 0);
            self.remote = self.control(
                "BUTTON",
                "Create public HTTPS link",
                48,
                282,
                292,
                24,
                REMOTE,
                BS_AUTOCHECKBOX as u32,
            );
            self.label(
                "Browser-only access beyond your Wi-Fi.\nRemote mode uses a local-only server.",
                48,
                320,
                292,
                42,
                self.style.small,
            );
            self.start = self.control(
                "BUTTON",
                "Start sharing",
                48,
                380,
                184,
                44,
                START,
                BS_DEFPUSHBUTTON as u32,
            );
            self.stop = self.control("BUTTON", "Stop", 244, 380, 104, 44, STOP, 0);
            EnableWindow(self.stop, 0);
            self.label("Remote access", 412, 120, 296, 28, self.style.heading);
            self.label(
                "Cloudflare TURN key ID",
                412,
                162,
                296,
                20,
                self.style.small,
            );
            self.key = self.control(
                "EDIT",
                &std::env::var("SCREEN_SHARE_TURN_KEY_ID").unwrap_or_default(),
                422,
                196,
                280,
                22,
                KEY,
                ES_AUTOHSCROLL as u32,
            );
            self.label("TURN API token", 412, 240, 296, 20, self.style.small);
            self.secret = self.control(
                "EDIT",
                &std::env::var("SCREEN_SHARE_TURN_API_TOKEN").unwrap_or_default(),
                422,
                274,
                280,
                22,
                SECRET,
                ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32,
            );
            self.remember = self.control(
                "BUTTON",
                "Remember credentials on this PC",
                412,
                322,
                296,
                24,
                REMEMBER,
                BS_AUTOCHECKBOX as u32,
            );
            SendMessageW(self.remember, BM_SETCHECK, BST_CHECKED as usize, 0);
            self.label(
                "Protected by Windows Credential Manager.",
                412,
                352,
                296,
                20,
                self.style.small,
            );
            self.forget =
                self.control("BUTTON", "Forget credentials", 412, 386, 166, 38, FORGET, 0);
            self.control("BUTTON", "Setup guide", 590, 386, 122, 38, HELP, 0);
            self.label("Viewer link", 48, 488, 200, 26, self.style.heading);
            self.status = self.control(
                "STATIC",
                "Ready when you are. Start sharing to create a private link.",
                48,
                523,
                664,
                22,
                0,
                0,
            );
            self.link = self.control(
                "EDIT",
                "",
                58,
                570,
                458,
                22,
                LINK,
                ES_AUTOHSCROLL as u32 | ES_READONLY as u32,
            );
            let copy = self.control("BUTTON", "Copy link", 540, 560, 100, 42, COPY, 0);
            let open = self.control("BUTTON", "Open", 652, 560, 60, 42, OPEN, 0);
            EnableWindow(copy, 0);
            EnableWindow(open, 0);
            SendMessageW(
                self.link,
                EM_SETCUEBANNER,
                0,
                wide("Your private viewer link appears here").as_ptr() as isize,
            );
            self.label(
                "1080p / 60 fps  ·  One viewer  ·  Minimize to tray",
                32,
                644,
                450,
                20,
                self.style.small,
            );
            self.diagnostics =
                self.control("BUTTON", "Diagnostics", 496, 634, 136, 34, DIAGNOSTICS, 0);
            self.control("BUTTON", "Exit", 644, 634, 92, 34, EXIT, 0);
            self.log = self.control(
                "EDIT",
                "",
                48,
                702,
                664,
                76,
                LOG,
                ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_READONLY as u32 | WS_VSCROLL,
            );
            self.style.font(self.log, self.style.small);
            ShowWindow(self.log, SW_HIDE);
            match credentials::load(credentials::TARGET) {
                Ok(Some(saved)) => {
                    // An explicit complete environment pair overrides stored values.
                    if std::env::var("SCREEN_SHARE_TURN_KEY_ID").unwrap_or_default().is_empty()
                        || std::env::var("SCREEN_SHARE_TURN_API_TOKEN").unwrap_or_default().is_empty()
                    {
                        Self::set(self.key, &saved.key);
                        Self::set(self.secret, &saved.token);
                    }
                    self.note("Saved TURN credentials loaded from Windows Credential Manager.");
                }
                Ok(None) => {}
                Err(_) => self.note("Could not load saved credentials. Enter them again or use Forget saved credentials."),
            }
            self.taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
            self.add_tray();
            SetTimer(self.hwnd, 1, 200, None);
        }
    }
    // Mirrors the native CreateWindow layout arguments at each control call site.
    #[allow(clippy::too_many_arguments)]
    fn control(
        &self,
        class: &str,
        title: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: usize,
        style: u32,
    ) -> HWND {
        unsafe {
            let is_button = class == "BUTTON" && style & 0xf != BS_AUTOCHECKBOX as u32;
            let style = if is_button && self.style.owner_draw() {
                (style & !0xf) | BS_OWNERDRAW as u32
            } else {
                style
            };
            let hwnd = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(title).as_ptr(),
                WS_CHILD | WS_VISIBLE | if class == "STATIC" { 0 } else { WS_TABSTOP } | style,
                self.style.px(x),
                self.style.px(y),
                self.style.px(width),
                self.style.px(height),
                self.hwnd,
                id as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            self.style.font(hwnd, self.style.body);
            if is_button {
                self.style.prepare_button(hwnd);
            } else if class == "BUTTON" {
                self.style.prepare_checkbox(hwnd);
            }
            hwnd
        }
    }
    fn label(&self, text: &str, x: i32, y: i32, width: i32, height: i32, font: HFONT) {
        let label = self.control("STATIC", text, x, y, width, height, 0, 0);
        self.style.font(label, font);
    }
    fn toggle_diagnostics(&mut self) {
        self.expanded = !self.expanded;
        Self::set(
            self.diagnostics,
            if self.expanded {
                "Hide diagnostics"
            } else {
                "Diagnostics"
            },
        );
        unsafe {
            ShowWindow(self.log, if self.expanded { SW_SHOW } else { SW_HIDE });
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: self.style.px(desktop_style::WIDTH),
                bottom: self.style.px(if self.expanded {
                    desktop_style::EXPANDED_HEIGHT
                } else {
                    desktop_style::HEIGHT
                }),
            };
            let window_style = GetWindowLongW(self.hwnd, GWL_STYLE) as u32;
            AdjustWindowRect(&mut rect, window_style, 0);
            let mut current = RECT::default();
            GetWindowRect(self.hwnd, &mut current);
            let mut monitor: MONITORINFO = std::mem::zeroed();
            monitor.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(
                MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            );
            let height = rect.bottom - rect.top;
            let top = current
                .top
                .min(monitor.rcWork.bottom - height)
                .max(monitor.rcWork.top);
            SetWindowPos(
                self.hwnd,
                null_mut(),
                current.left,
                top,
                rect.right - rect.left,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            InvalidateRect(self.hwnd, null(), 0);
        }
    }
    fn text(hwnd: HWND) -> String {
        unsafe {
            let len = GetWindowTextLengthW(hwnd).clamp(0, 8192) as usize;
            let mut buffer = vec![0; len + 1];
            let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
        }
    }
    fn set(hwnd: HWND, text: &str) {
        unsafe {
            SetWindowTextW(hwnd, wide(text).as_ptr());
        }
    }
    fn checked(hwnd: HWND) -> bool {
        unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize }
    }
    fn note(&mut self, line: &str) {
        self.lines.push_str(line);
        self.lines.push_str("\r\n");
        if self.lines.len() > 12000 {
            let start = self.lines.len() - 8000;
            let start = self
                .lines
                .char_indices()
                .find(|(index, character)| *index >= start && *character == '\n')
                .map(|(index, _)| index + 1)
                .unwrap_or(self.lines.len());
            self.lines = self.lines[start..].to_owned();
        }
        Self::set(self.log, &self.lines);
    }
    fn add_tray(&mut self) {
        unsafe {
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd;
            data.uID = 1;
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            data.uCallbackMessage = TRAY;
            data.hIcon = LoadIconW(GetModuleHandleW(null()), 1usize as _);
            let tip = wide("Monochrome - Share - double-click to open");
            data.szTip[..tip.len()].copy_from_slice(&tip);
            self.tray_added = Shell_NotifyIconW(NIM_ADD, &data) != 0;
        }
    }
    fn launch(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(self.host.is_none(), "Stop the current session first");
        self.generation += 1;
        self.token_fragment.clear();
        self.public_url.clear();
        Self::set(self.link, "");
        let remote = Self::checked(self.remote);
        let key = Self::text(self.key).trim().to_owned();
        let secret = Self::text(self.secret).trim().to_owned();
        anyhow::ensure!(
            !remote || (!key.is_empty() && !secret.is_empty()),
            "Enter the Cloudflare TURN key ID and API token. Remote setup explains how to obtain them."
        );
        let bind = if remote {
            "127.0.0.1:8000".to_owned()
        } else {
            Self::text(self.bind).trim().to_owned()
        };
        let address: SocketAddr = bind.parse().map_err(|_| {
            anyhow::anyhow!("Enter an IP address and port, such as 192.168.1.12:8000")
        })?;
        anyhow::ensure!(
            !address.ip().is_unspecified() && address.port() != 0,
            "Choose a specific local IP and a nonzero port"
        );
        let directory = std::env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Missing application directory"))?
            .to_owned();
        let tunnel_exe = if directory.join("cloudflared.exe").is_file() {
            directory.join("cloudflared.exe")
        } else {
            PathBuf::from("cloudflared.exe")
        };
        if remote {
            let status = Command::new(&tunnel_exe)
                .arg("--version")
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|_| anyhow::anyhow!("cloudflared.exe is missing. See Remote setup."))?;
            anyhow::ensure!(status.success(), "cloudflared could not start");
            if Self::checked(self.remember) {
                if credentials::save(
                    credentials::TARGET,
                    &credentials::Credentials {
                        key: key.clone(),
                        token: secret.clone(),
                    },
                )
                .is_ok()
                {
                    self.note("TURN credentials saved in Windows Credential Manager.");
                } else {
                    self.note("Could not save TURN credentials. Sharing will continue; credentials may need re-entry next time.");
                }
            } else {
                credentials::forget(credentials::TARGET).map_err(|_| {
                    anyhow::anyhow!("Could not remove saved credentials; sharing has not started")
                })?;
            }
        }
        unsafe {
            self.job = CreateJobObjectW(null(), null());
            anyhow::ensure!(!self.job.is_null(), "Could not create the process group");
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                self.job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                CloseHandle(self.job);
                self.job = null_mut();
                anyhow::bail!("Could not configure the process group");
            }
        }
        let mut command = Command::new(directory.join("monochrome-capture.exe"));
        command.args([
            "--desktop-child",
            "--bind",
            &bind,
            "--audio",
            if Self::checked(self.audio) {
                "desktop"
            } else {
                "off"
            },
        ]);
        command
            .env_remove("SCREEN_SHARE_TOKEN")
            .env_remove("SCREEN_SHARE_TURN_KEY_ID")
            .env_remove("SCREEN_SHARE_TURN_API_TOKEN");
        if remote {
            command
                .arg("--remote")
                .env("SCREEN_SHARE_TURN_KEY_ID", key)
                .env("SCREEN_SHARE_TURN_API_TOKEN", secret);
        }
        let runtime = if directory.join("runtime/bin").is_dir() {
            directory.join("runtime")
        } else {
            std::env::var_os("GSTREAMER_1_0_ROOT_MSVC_X86_64")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("C:\\Program Files\\gstreamer\\1.0\\msvc_x86_64"))
        };
        let mut paths = vec![runtime.join("bin")];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }
        command
            .env("PATH", std::env::join_paths(paths)?)
            .env_remove("GST_PLUGIN_PATH")
            .env("GST_PLUGIN_SYSTEM_PATH", runtime.join("lib/gstreamer-1.0"));
        command.env(
            "GST_PLUGIN_SCANNER",
            runtime.join("libexec/gstreamer-1.0/gst-plugin-scanner.exe"),
        );
        let child = self.spawn(&mut command, false)?;
        self.host = Some(child);
        if remote {
            let mut tunnel = Command::new(tunnel_exe);
            tunnel.args([
                "tunnel",
                "--no-autoupdate",
                "--url",
                "http://127.0.0.1:8000",
            ]);
            match self.spawn(&mut tunnel, true) {
                Ok(child) => self.tunnel = Some(child),
                Err(error) => {
                    self.request_stop();
                    return Err(error);
                }
            }
        }
        self.stopping = None;
        unsafe {
            EnableWindow(self.start, 0);
            EnableWindow(self.stop, 1);
        }
        for control in [
            self.bind,
            self.audio,
            self.remote,
            self.key,
            self.secret,
            self.remember,
            self.forget,
        ] {
            unsafe {
                EnableWindow(control, 0);
            }
        }
        Self::set(self.status, "Starting host...");
        Ok(())
    }
    fn spawn(&self, command: &mut Command, tunnel: bool) -> anyhow::Result<Child> {
        let mut child = command
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if unsafe { AssignProcessToJobObject(self.job, child.as_raw_handle() as HANDLE) } == 0 {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Could not attach the child process to the desktop app");
        }
        fn reader<R: std::io::Read + Send + 'static>(
            pipe: R,
            tx: Sender<Event>,
            tunnel: bool,
            generation: u64,
        ) {
            std::thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx
                        .send(if tunnel {
                            Event::Tunnel(generation, line)
                        } else {
                            Event::Host(generation, line)
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        if let Some(pipe) = child.stdout.take() {
            reader(pipe, self.tx.clone(), tunnel, self.generation);
        }
        if let Some(pipe) = child.stderr.take() {
            reader(pipe, self.tx.clone(), tunnel, self.generation);
        }
        Ok(child)
    }
    fn request_stop(&mut self) {
        if let Some(host) = &mut self.host {
            drop(host.stdin.take());
        }
        if let Some(tunnel) = &mut self.tunnel {
            let _ = tunnel.kill();
        }
        self.stopping = Some(Instant::now());
        Self::set(self.status, "Stopping sharing...");
        Self::set(self.link, "");
        self.token_fragment.clear();
        self.public_url.clear();
    }
    fn cleanup(&mut self) {
        self.generation += 1;
        for child in [&mut self.host, &mut self.tunnel] {
            if let Some(mut child) = child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if !self.job.is_null() {
            unsafe {
                CloseHandle(self.job);
            }
            self.job = null_mut();
        }
        self.stopping = None;
        self.token_fragment.clear();
        self.public_url.clear();
        Self::set(self.link, "");
        unsafe {
            EnableWindow(self.start, 1);
            EnableWindow(self.stop, 0);
        }
        for control in [
            self.bind,
            self.audio,
            self.remote,
            self.key,
            self.secret,
            self.remember,
            self.forget,
        ] {
            unsafe {
                EnableWindow(control, 1);
            }
        }
        Self::set(
            self.status,
            "Stopped. Start sharing to generate a new private link.",
        );
    }
    fn tick(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            if self.stopping.is_some() || self.host.is_none() {
                continue;
            }
            match event {
                Event::Host(generation, line) => {
                    if generation != self.generation {
                        continue;
                    }
                    if let Some(link) = line.strip_prefix("Viewer: ") {
                        if let Some((_, fragment)) = link.split_once('#') {
                            self.token_fragment = fragment.to_owned();
                            if !Self::checked(self.remote) {
                                Self::set(self.link, link);
                            }
                        }
                        Self::set(
                            self.status,
                            if Self::checked(self.remote) {
                                "Host ready. Waiting for HTTPS tunnel..."
                            } else {
                                "Sharing enabled. Waiting for a viewer."
                            },
                        );
                    } else if line.starts_with("Status: viewer-connected") {
                        Self::set(
                            self.status,
                            "Viewer joined. Stream is negotiating or active.",
                        );
                    } else if line.starts_with("Status: viewer-disconnected") {
                        Self::set(self.status, "Sharing enabled. Viewer disconnected.");
                    } else {
                        self.note(&line);
                    }
                }
                Event::Tunnel(generation, line) => {
                    if generation != self.generation {
                        continue;
                    }
                    if let Some(url) = tunnel_url(&line) {
                        self.public_url = url;
                    }
                    // Tunnel logs do not contain the private viewer fragment.
                    self.note(&line);
                }
            }
            if Self::checked(self.remote)
                && !self.public_url.is_empty()
                && !self.token_fragment.is_empty()
            {
                let link = format!("{}/#{}", self.public_url, self.token_fragment);
                if Self::text(self.link) != link {
                    Self::set(self.link, &link);
                    Self::set(self.status, "Public link ready. Waiting for a viewer.");
                }
            }
        }
        let host_ended = self
            .host
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().flatten().is_some());
        let tunnel_ended = self
            .tunnel
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().flatten().is_some());
        if host_ended
            || tunnel_ended
            || self
                .stopping
                .is_some_and(|t| t.elapsed() > Duration::from_secs(3))
        {
            if self.stopping.is_none() {
                self.note("Host or tunnel stopped unexpectedly. Check the messages above.");
            }
            self.cleanup();
        }
        let has_link = !Self::text(self.link).is_empty();
        unsafe {
            EnableWindow(GetDlgItem(self.hwnd, COPY as i32), has_link as i32);
            EnableWindow(GetDlgItem(self.hwnd, OPEN as i32), has_link as i32);
        }
    }
    fn command(&mut self, id: usize) {
        match id {
            START => {
                if let Err(error) = self.launch() {
                    self.note(&format!("{error:#}"));
                    if self.host.is_none() {
                        self.cleanup();
                    }
                    Self::set(self.status, &format!("Cannot start: {error}"));
                }
            }
            STOP => self.request_stop(),
            DIAGNOSTICS => self.toggle_diagnostics(),
            REMEMBER | FORGET if self.host.is_none() => {
                if id == FORGET || !Self::checked(self.remember) {
                    match credentials::forget(credentials::TARGET) {
                        Ok(()) => {
                            unsafe {
                                SendMessageW(self.remember, BM_SETCHECK, BST_UNCHECKED as usize, 0);
                            }
                            if id == FORGET {
                                Self::set(self.key, "");
                                Self::set(self.secret, "");
                            }
                            self.note("Saved TURN credentials removed from this PC. Existing sessions or provider tokens are not revoked.");
                        }
                        Err(_) => {
                            unsafe {
                                SendMessageW(self.remember, BM_SETCHECK, BST_CHECKED as usize, 0);
                            }
                            self.note("Could not remove saved credentials. Try again or remove MonochromeCapture/CloudflareTURN/v1 in Windows Credential Manager.");
                        }
                    }
                }
            }
            COPY => {
                let link = Self::text(self.link);
                if !link.is_empty() && !copy_text(self.hwnd, &link) {
                    self.note("Clipboard unavailable. Select and copy the link manually.");
                }
            }
            OPEN => {
                let link = Self::text(self.link);
                if !link.is_empty() {
                    open(&link);
                }
            }
            HELP => open("https://developers.cloudflare.com/realtime/turn/generate-credentials/"),
            SHOW => unsafe {
                ShowWindow(self.hwnd, SW_RESTORE);
                SetForegroundWindow(self.hwnd);
            },
            EXIT => {
                self.cleanup();
                unsafe {
                    DestroyWindow(self.hwnd);
                }
            }
            _ => {}
        }
    }
}

fn tunnel_url(line: &str) -> Option<String> {
    line.split_whitespace().find_map(|part| {
        let part = part.trim_matches(|c: char| matches!(c, '|' | '"' | ','));
        part.strip_prefix("https://")
            .filter(|host| {
                host.ends_with(".trycloudflare.com")
                    && host
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            })
            .map(|_| part.to_owned())
    })
}
fn open(url: &str) {
    unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            wide(url).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}
fn copy_text(hwnd: HWND, text: &str) -> bool {
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return false;
        }
        let content = wide(text);
        let memory = GlobalAlloc(GMEM_MOVEABLE, content.len() * 2);
        let ptr = if memory.is_null() {
            null_mut()
        } else {
            GlobalLock(memory)
        };
        let success = if ptr.is_null() {
            false
        } else {
            std::ptr::copy_nonoverlapping(content.as_ptr(), ptr as *mut u16, content.len());
            GlobalUnlock(memory);
            EmptyClipboard();
            !SetClipboardData(13, memory).is_null()
        };
        if !success && !memory.is_null() {
            GlobalFree(memory);
        }
        CloseClipboard();
        success
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
        if !pointer.is_null() {
            let state = &*pointer;
            // Painting must also work during synchronous native calls while App
            // is mutably borrowed; the shared style has no mutable UI state.
            match message {
                WM_DRAWITEM if lparam != 0 => {
                    state.style.draw_button(&*(lparam as *const DRAWITEMSTRUCT));
                    return 1;
                }
                WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => {
                    let control = lparam as HWND;
                    let id = GetDlgCtrlID(control);
                    return state.style.color(
                        hwnd,
                        control,
                        wparam as HDC,
                        (BIND as i32..=LOG as i32).contains(&id),
                    );
                }
                WM_ERASEBKGND => return 1,
                _ => {}
            }
            // Native operations may re-enter the window procedure. Skip state work
            // while already borrowed, avoiding aliased mutable references.
            if let Ok(mut app) = state.app.try_borrow_mut() {
                if message == app.taskbar_created {
                    app.add_tray();
                    return 0;
                }
                match message {
                    READY => return 1,
                    WM_PAINT => {
                        app.style.paint(
                            hwnd,
                            &[app.bind, app.key, app.secret, app.link, app.log],
                            app.expanded,
                        );
                        return 0;
                    }
                    WM_COMMAND => {
                        let notification = (wparam >> 16) as u32;
                        if notification == EN_SETFOCUS || notification == EN_KILLFOCUS {
                            InvalidateRect(hwnd, null(), 0);
                        }
                        // Edit notifications must not execute button commands.
                        if notification == BN_CLICKED {
                            app.command(wparam & 0xffff);
                        }
                        return 0;
                    }
                    WM_TIMER => {
                        app.tick();
                        return 0;
                    }
                    WM_SIZE if wparam == SIZE_MINIMIZED as usize && app.tray_added => {
                        ShowWindow(hwnd, SW_HIDE);
                        return 0;
                    }
                    WM_CLOSE => {
                        if app.tray_added {
                            ShowWindow(hwnd, SW_HIDE);
                        } else {
                            app.cleanup();
                            DestroyWindow(hwnd);
                        }
                        return 0;
                    }
                    TRAY if lparam as u32 == WM_LBUTTONDBLCLK => {
                        app.command(SHOW);
                        return 0;
                    }
                    TRAY if lparam as u32 == WM_RBUTTONUP => {
                        let menu = CreatePopupMenu();
                        AppendMenuW(
                            menu,
                            MF_STRING,
                            SHOW,
                            wide("Open Monochrome - Share").as_ptr(),
                        );
                        AppendMenuW(menu, MF_STRING, STOP, wide("Stop sharing").as_ptr());
                        AppendMenuW(menu, MF_STRING, EXIT, wide("Exit").as_ptr());
                        let mut point: POINT = std::mem::zeroed();
                        GetCursorPos(&mut point);
                        SetForegroundWindow(hwnd);
                        let command = TrackPopupMenu(
                            menu,
                            TPM_RETURNCMD | TPM_NONOTIFY,
                            point.x,
                            point.y,
                            0,
                            hwnd,
                            null(),
                        ) as usize;
                        DestroyMenu(menu);
                        app.command(command);
                        return 0;
                    }
                    _ => {}
                }
            }
        }
        if message == WM_DESTROY {
            PostQuitMessage(0);
            return 0;
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

pub fn run() {
    // Also recognize an already-running older build that predates the mutex.
    if activate_existing_instance(false) {
        return;
    }
    let _instance_guard = match InstanceGuard::acquire(INSTANCE_MUTEX) {
        Ok(Some(guard)) => guard,
        Ok(None) => {
            activate_existing_instance(true);
            return;
        }
        Err(error) => {
            unsafe {
                MessageBoxW(
                    null_mut(),
                    wide(&format!("Could not enforce a single app instance: {error}")).as_ptr(),
                    wide("Monochrome - Share").as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
            return;
        }
    };
    unsafe {
        SetProcessDPIAware();
        let instance = GetModuleHandleW(null());
        let name = wide(WINDOW_CLASS);
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: name.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(instance, 1usize as _),
            hbrBackground: null_mut(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return;
        }
        let app = App::new();
        let state = Box::new(WindowState {
            style: Rc::clone(&app.style),
            app: RefCell::new(app),
        });
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: state.style.px(desktop_style::WIDTH),
            bottom: state.style.px(desktop_style::HEIGHT),
        };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        AdjustWindowRect(&mut rect, style, 0);
        let hwnd = CreateWindowExW(
            0,
            name.as_ptr(),
            wide("Monochrome - Share").as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*state as *const WindowState as isize);
        {
            let mut app = state.app.borrow_mut();
            app.hwnd = hwnd;
            app.initialize();
        }
        ShowWindow(hwnd, SW_SHOW);
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let mut app = state.app.borrow_mut();
        app.cleanup();
        let mut icon: NOTIFYICONDATAW = std::mem::zeroed();
        icon.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        icon.hWnd = hwnd;
        icon.uID = 1;
        Shell_NotifyIconW(NIM_DELETE, &icon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instance_guard_blocks_another_thread_and_releases_on_drop() {
        let name = format!("Local\\MonochromeCapture.Test.{}", std::process::id());
        let guard = InstanceGuard::acquire(&name).unwrap().unwrap();
        let second_name = name.clone();
        assert!(
            std::thread::spawn(move || { InstanceGuard::acquire(&second_name).unwrap().is_none() })
                .join()
                .unwrap()
        );
        drop(guard);
        assert!(
            std::thread::spawn(move || { InstanceGuard::acquire(&name).unwrap().is_some() })
                .join()
                .unwrap()
        );
    }
    #[test]
    fn instance_guard_recovers_an_abandoned_owner() {
        let name = format!(
            "Local\\MonochromeCapture.TestAbandoned.{}",
            std::process::id()
        );
        let abandoned_name = name.clone();
        let abandoned_handle = std::thread::spawn(move || {
            let guard = InstanceGuard::acquire(&abandoned_name).unwrap().unwrap();
            let handle = guard.0 as usize;
            // Model a thread/process exiting without its Rust cleanup running.
            std::mem::forget(guard);
            handle
        })
        .join()
        .unwrap();
        let recovered = InstanceGuard::acquire(&name).unwrap().unwrap();
        unsafe { CloseHandle(abandoned_handle as HANDLE) };
        drop(recovered);
        assert!(InstanceGuard::acquire(&name).unwrap().is_some());
    }

    #[test]
    fn simultaneous_instance_attempts_have_one_owner() {
        let name = format!("Local\\MonochromeCapture.TestRace.{}", std::process::id());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let name = name.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    let guard = InstanceGuard::acquire(&name).unwrap();
                    let owns = guard.is_some();
                    barrier.wait();
                    drop(guard);
                    owns
                })
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|thread| usize::from(thread.join().unwrap()))
                .sum::<usize>(),
            1
        );
    }
    #[test]
    fn recognizes_only_valid_quick_tunnel_addresses() {
        assert_eq!(
            tunnel_url("| https://quiet-sky.trycloudflare.com |"),
            Some("https://quiet-sky.trycloudflare.com".into())
        );
        assert!(tunnel_url("https://example.com").is_none());
        assert!(tunnel_url("https://bad.trycloudflare.com/evil").is_none());
    }
}
