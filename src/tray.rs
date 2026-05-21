#[cfg(windows)]
mod windows_tray {
    use std::ffi::{OsStr, c_void};
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use crate::app::{self, RuntimeOptions, TweakOptions};
    use crate::error::{Error, Result};
    use crate::platform::{DisplayPlatform, SystemDisplayPlatform};
    use crate::updates::{self, UpdateCheck};

    const APP_NAME: &str = "Color LUT Tweaks";
    const WM_APP: u32 = 0x8000;
    const WM_TRAY_ICON: u32 = WM_APP + 1;
    const WM_WORKER_DONE: u32 = WM_APP + 2;
    const WM_UPDATE_CHECKED: u32 = WM_APP + 3;
    const WM_DESTROY: u32 = 0x0002;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const GWLP_USERDATA: i32 = -21;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const ERROR_ALREADY_EXISTS: u32 = 183;

    const NIM_ADD: u32 = 0x00000000;
    const NIM_DELETE: u32 = 0x00000002;
    const NIF_MESSAGE: u32 = 0x00000001;
    const NIF_ICON: u32 = 0x00000002;
    const NIF_TIP: u32 = 0x00000004;

    const MF_STRING: u32 = 0x00000000;
    const MF_CHECKED: u32 = 0x00000008;
    const MF_GRAYED: u32 = 0x00000001;
    const MF_POPUP: u32 = 0x00000010;
    const MF_SEPARATOR: u32 = 0x00000800;
    const TPM_RETURNCMD: u32 = 0x00000100;
    const TPM_RIGHTBUTTON: u32 = 0x00000002;

    const MB_ICONERROR: u32 = 0x00000010;
    const IMAGE_ICON: u32 = 1;
    const IDI_APPLICATION: usize = 32512;
    const LR_LOADFROMFILE: u32 = 0x00000010;
    const LR_DEFAULTSIZE: u32 = 0x00000040;
    const SW_SHOWNORMAL: i32 = 1;

    const TRAY_UID: u32 = 1;
    const MENU_OPEN_EXPLORER: usize = 1001;
    const MENU_OPEN_CONFIG: usize = 1002;
    const MENU_ENABLED: usize = 1003;
    const MENU_FORCE: usize = 1004;
    const MENU_RELOAD: usize = 1005;
    const MENU_STARTUP: usize = 1006;
    const MENU_QUIT: usize = 1007;
    const MENU_UPDATE: usize = 1008;
    const MENU_PRESET_BASE: usize = 2000;
    const INSTANCE_MUTEX_NAME: &str = "Local\\ColorLutTweaksTray";
    const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
    const UPDATE_CHECK_WAIT_SLICE: Duration = Duration::from_secs(1);

    pub fn launch(config: Option<PathBuf>) -> Result<()> {
        if instance_running() {
            return Ok(());
        }

        let exe = std::env::current_exe().map_err(|source| Error::Io { path: None, source })?;
        let mut command = Command::new(exe);
        command.arg("tray-worker");

        if let Some(config) = config {
            command.arg("--config").arg(config);
        }

        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|source| Error::Io { path: None, source })?;

        Ok(())
    }

    pub fn run(config: Option<PathBuf>) -> Result<()> {
        let Some(_instance) = SingleInstance::acquire()? else {
            return Ok(());
        };

        let settings_path = default_settings_path()?;
        let mut settings = TraySettings::load(&settings_path)?;
        settings.preset = resolve_existing_preset(&settings.preset)?;
        settings.start_with_windows = crate::startup::enabled().unwrap_or(false);
        settings.save(&settings_path)?;
        let config = config.unwrap_or_else(|| preset_config_path(&settings.preset));

        unsafe {
            let hwnd = create_window()?;
            let mut state = Box::new(TrayState {
                enabled: settings.enabled,
                force: settings.force,
                startup_enabled: settings.start_with_windows,
                worker: None,
                update_worker: Some(UpdateWorker::start(hwnd)),
                config,
                settings_path,
                settings,
            });
            if state.enabled {
                state.start_worker(hwnd)?;
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
            add_tray_icon(hwnd)?;
            message_loop()?;
        }

        Ok(())
    }

    struct TrayState {
        enabled: bool,
        force: bool,
        startup_enabled: bool,
        worker: Option<RuntimeWorker>,
        update_worker: Option<UpdateWorker>,
        config: PathBuf,
        settings_path: PathBuf,
        settings: TraySettings,
    }

    struct RuntimeWorker {
        shutdown: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
        result: mpsc::Receiver<Result<()>>,
    }

    struct UpdateWorker {
        shutdown: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
        status: Arc<Mutex<CachedUpdateStatus>>,
    }

    #[derive(Clone, Debug)]
    enum CachedUpdateStatus {
        Checking,
        Latest,
        Available { version: String, url: String },
        Failed,
    }

    impl UpdateWorker {
        fn start(hwnd: Hwnd) -> Self {
            let shutdown = Arc::new(AtomicBool::new(false));
            let status = Arc::new(Mutex::new(CachedUpdateStatus::Checking));
            let thread_shutdown = shutdown.clone();
            let thread_status = status.clone();
            let hwnd_value = hwnd as isize;
            let handle = thread::spawn(move || {
                loop {
                    if let Ok(mut status) = thread_status.lock() {
                        *status = CachedUpdateStatus::Checking;
                    }

                    let next_status = match updates::check_latest() {
                        Ok(UpdateCheck::Latest) => CachedUpdateStatus::Latest,
                        Ok(UpdateCheck::Available { version, url }) => {
                            CachedUpdateStatus::Available { version, url }
                        }
                        Err(_) => CachedUpdateStatus::Failed,
                    };

                    if let Ok(mut status) = thread_status.lock() {
                        *status = next_status;
                    }
                    unsafe {
                        PostMessageW(hwnd_value as Hwnd, WM_UPDATE_CHECKED, 0, 0);
                    }

                    if !wait_for_next_update_check(&thread_shutdown) {
                        break;
                    }
                }
            });

            Self {
                shutdown,
                handle: Some(handle),
                status,
            }
        }

        fn status(&self) -> CachedUpdateStatus {
            self.status
                .lock()
                .map(|status| status.clone())
                .unwrap_or(CachedUpdateStatus::Failed)
        }

        fn stop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn wait_for_next_update_check(shutdown: &AtomicBool) -> bool {
        let mut waited = Duration::ZERO;
        while waited < UPDATE_CHECK_INTERVAL {
            if shutdown.load(Ordering::Relaxed) {
                return false;
            }
            thread::sleep(UPDATE_CHECK_WAIT_SLICE);
            waited += UPDATE_CHECK_WAIT_SLICE;
        }

        !shutdown.load(Ordering::Relaxed)
    }

    impl TrayState {
        fn start_worker(&mut self, hwnd: Hwnd) -> Result<()> {
            let tweaks = TweakOptions::list_from_config_file(&self.config)?;
            let shutdown = Arc::new(AtomicBool::new(false));
            let (tx, rx) = mpsc::channel();
            let hwnd_value = hwnd as isize;
            let thread_shutdown = shutdown.clone();
            let force = self.force;
            let handle = thread::spawn(move || {
                let platform = SystemDisplayPlatform::new();
                let result =
                    app::run_tweaks_until(&platform, &tweaks, RuntimeOptions { force }, || {
                        thread_shutdown.load(Ordering::Relaxed)
                    });
                let _ = tx.send(result);
                unsafe {
                    PostMessageW(hwnd_value as Hwnd, WM_WORKER_DONE, 0, 0);
                }
            });

            self.worker = Some(RuntimeWorker {
                shutdown,
                handle: Some(handle),
                result: rx,
            });

            Ok(())
        }

        fn stop_worker(&mut self) -> Result<()> {
            let Some(mut worker) = self.worker.take() else {
                return Ok(());
            };

            worker.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
            }

            match worker.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => Ok(()),
            }
        }

        fn toggle_enabled(&mut self, hwnd: Hwnd) -> Result<()> {
            if self.enabled {
                self.stop_worker()?;
                self.enabled = false;
            } else {
                self.start_worker(hwnd)?;
                self.enabled = true;
            }

            self.settings.enabled = self.enabled;
            self.save_settings()?;
            Ok(())
        }

        fn toggle_force(&mut self, hwnd: Hwnd) -> Result<()> {
            self.force = !self.force;
            self.settings.force = self.force;
            self.save_settings()?;
            if self.enabled {
                self.reload(hwnd)?;
            }

            Ok(())
        }

        fn toggle_startup(&mut self) -> Result<()> {
            if crate::startup::enabled()? {
                crate::startup::disable()?;
                self.startup_enabled = false;
            } else {
                crate::startup::enable()?;
                self.startup_enabled = true;
            }

            self.settings.start_with_windows = self.startup_enabled;
            self.save_settings()?;
            Ok(())
        }

        fn select_preset(&mut self, hwnd: Hwnd, preset: String) -> Result<()> {
            if self.settings.preset == preset {
                return Ok(());
            }

            self.settings.preset = preset;
            self.config = preset_config_path(&self.settings.preset);
            self.save_settings()?;
            self.reload(hwnd)
        }

        fn reload(&mut self, hwnd: Hwnd) -> Result<()> {
            self.stop_worker()?;
            if self.enabled
                && let Err(err) = self.start_worker(hwnd)
            {
                self.enabled = false;
                self.settings.enabled = false;
                let _ = self.save_settings();
                return Err(err);
            }

            Ok(())
        }

        fn save_settings(&self) -> Result<()> {
            self.settings.save(&self.settings_path)
        }

        fn handle_worker_done(&mut self) -> Result<()> {
            let Some(mut worker) = self.worker.take() else {
                return Ok(());
            };

            let result = match worker.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    self.worker = Some(worker);
                    return Ok(());
                }
                Err(mpsc::TryRecvError::Disconnected) => Err(Error::platform(
                    "the background runtime stopped unexpectedly",
                )),
            };

            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
            }
            self.enabled = false;
            result
        }

        fn update_status(&self) -> CachedUpdateStatus {
            self.update_worker
                .as_ref()
                .map(UpdateWorker::status)
                .unwrap_or(CachedUpdateStatus::Failed)
        }

        fn update_url(&self) -> Option<String> {
            match self.update_status() {
                CachedUpdateStatus::Available { url, .. } => Some(url),
                CachedUpdateStatus::Latest => Some(updates::RELEASES_PAGE_URL.to_string()),
                _ => None,
            }
        }
    }

    unsafe fn create_window() -> Result<Hwnd> {
        let class_name = wide("ColorLutTweaksTrayWindow");
        let window_name = wide(APP_NAME);
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        let class = WndClassExW {
            cb_size: size_of::<WndClassExW>() as u32,
            style: 0,
            lpfn_wnd_proc: Some(window_proc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: instance,
            h_icon: ptr::null_mut(),
            h_cursor: ptr::null_mut(),
            hbr_background: ptr::null_mut(),
            lpsz_menu_name: ptr::null(),
            lpsz_class_name: class_name.as_ptr(),
            h_icon_sm: ptr::null_mut(),
        };

        if unsafe { RegisterClassExW(&class) } == 0 {
            return Err(last_os_error("RegisterClassExW failed"));
        }

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null_mut(),
            )
        };

        if hwnd.is_null() {
            return Err(last_os_error("CreateWindowExW failed"));
        }

        Ok(hwnd)
    }

    unsafe fn message_loop() -> Result<()> {
        let mut message = Msg::default();
        loop {
            let status = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
            if status == -1 {
                return Err(last_os_error("GetMessageW failed"));
            }
            if status == 0 {
                return Ok(());
            }

            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        match message {
            WM_TRAY_ICON
                if lparam as u32 == WM_RBUTTONUP
                    || lparam as u32 == WM_LBUTTONUP
                    || lparam as u32 == WM_CONTEXTMENU =>
            {
                unsafe {
                    show_menu(hwnd);
                }
                return 0;
            }
            WM_WORKER_DONE => unsafe {
                handle_worker_done(hwnd);
                return 0;
            },
            WM_UPDATE_CHECKED => {
                return 0;
            }
            WM_DESTROY => unsafe {
                cleanup(hwnd);
                PostQuitMessage(0);
                return 0;
            },
            _ => {}
        }

        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    unsafe fn show_menu(hwnd: Hwnd) {
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return;
        }

        let devices_header = wide("Devices");
        let help_header = wide("Help: Tool is not working?");
        let help_auto_color = wide("Disable Windows \"Auto Color Management\"");
        let help_nvidia_reference = wide("Disable NVIDIA \"Override to reference mode\"");
        let directory_header = wide("Directory");
        let open_explorer = wide("Open In Explorer");
        let open_config_label = wide("Open Configuration File");
        let color_header = wide("Color Adjustments");
        let presets = preset_items().unwrap_or_else(|_| Vec::new());
        let presets_label = wide("Presets");
        let enabled = wide("Enabled");
        let force = wide("Force");
        let reload = wide("Reload");
        let application_header = wide(format!("Application (v{})", env!("CARGO_PKG_VERSION")));
        let update_status = unsafe {
            state(hwnd)
                .map(|state| state.update_status())
                .unwrap_or(CachedUpdateStatus::Failed)
        };
        let (update_label, update_flags) = update_menu_item(&update_status);
        let update_label = wide(update_label);
        let startup = wide("Start with Windows");
        let quit = wide("Quit");
        let (enabled_flags, force_flags, startup_flags) =
            if let Some(state) = unsafe { state(hwnd) } {
                state.startup_enabled = crate::startup::enabled().unwrap_or(false);
                (
                    checked_flag(state.enabled),
                    checked_flag(state.force),
                    checked_flag(state.startup_enabled),
                )
            } else {
                (MF_STRING, MF_STRING, MF_STRING)
            };
        unsafe {
            AppendMenuW(menu, section_header_flags(), 0, devices_header.as_ptr());
            append_device_rows(menu);
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(menu, section_header_flags(), 0, help_header.as_ptr());
            AppendMenuW(menu, section_header_flags(), 0, help_auto_color.as_ptr());
            AppendMenuW(
                menu,
                section_header_flags(),
                0,
                help_nvidia_reference.as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(menu, section_header_flags(), 0, directory_header.as_ptr());
            AppendMenuW(menu, MF_STRING, MENU_OPEN_EXPLORER, open_explorer.as_ptr());
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_OPEN_CONFIG,
                open_config_label.as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(menu, section_header_flags(), 0, color_header.as_ptr());
            let presets_menu = CreatePopupMenu();
            if !presets_menu.is_null() {
                if presets.is_empty() {
                    let empty = wide("(none)");
                    AppendMenuW(presets_menu, section_header_flags(), 0, empty.as_ptr());
                }

                if let Some(state) = state(hwnd) {
                    for (index, preset) in presets.iter().enumerate() {
                        let label = wide(&preset.name);
                        AppendMenuW(
                            presets_menu,
                            checked_flag(state.settings.preset == preset.name),
                            MENU_PRESET_BASE + index,
                            label.as_ptr(),
                        );
                    }
                }

                AppendMenuW(
                    menu,
                    MF_STRING | MF_POPUP,
                    presets_menu as usize,
                    presets_label.as_ptr(),
                );
            }
            AppendMenuW(menu, enabled_flags, MENU_ENABLED, enabled.as_ptr());
            AppendMenuW(menu, force_flags, MENU_FORCE, force.as_ptr());
            AppendMenuW(menu, MF_STRING, MENU_RELOAD, reload.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(menu, section_header_flags(), 0, application_header.as_ptr());
            AppendMenuW(menu, update_flags, MENU_UPDATE, update_label.as_ptr());
            AppendMenuW(menu, startup_flags, MENU_STARTUP, startup.as_ptr());
            AppendMenuW(menu, MF_STRING, MENU_QUIT, quit.as_ptr());
        }

        let mut point = Point::default();
        unsafe {
            GetCursorPos(&mut point);
            SetForegroundWindow(hwnd);
        }
        let command = unsafe {
            TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                ptr::null(),
            )
        };
        unsafe {
            DestroyMenu(menu);
        }

        let command = command as usize;
        if (MENU_PRESET_BASE..MENU_PRESET_BASE + presets.len()).contains(&command) {
            unsafe {
                if let Some(state) = state(hwnd)
                    && let Some(preset) = presets.get(command - MENU_PRESET_BASE)
                    && let Err(err) = state.select_preset(hwnd, preset.name.clone())
                {
                    show_error(hwnd, &err.to_string());
                }
            }
            return;
        }

        match command {
            MENU_OPEN_EXPLORER => unsafe {
                if let Err(err) = open_in_explorer() {
                    show_error(hwnd, &err.to_string());
                }
            },
            MENU_OPEN_CONFIG => unsafe {
                if let Some(state) = state(hwnd)
                    && let Err(err) = open_config(&state.config)
                {
                    show_error(hwnd, &err.to_string());
                }
            },
            MENU_ENABLED => unsafe {
                if let Some(state) = state(hwnd)
                    && let Err(err) = state.toggle_enabled(hwnd)
                {
                    show_error(hwnd, &err.to_string());
                }
            },
            MENU_FORCE => unsafe {
                if let Some(state) = state(hwnd)
                    && let Err(err) = state.toggle_force(hwnd)
                {
                    show_error(hwnd, &err.to_string());
                }
            },
            MENU_STARTUP => unsafe {
                if let Some(state) = state(hwnd)
                    && let Err(err) = state.toggle_startup()
                {
                    show_error(hwnd, &err.to_string());
                }
            },
            MENU_RELOAD => unsafe {
                if let Some(state) = state(hwnd)
                    && let Err(err) = state.reload(hwnd)
                {
                    show_error(hwnd, &err.to_string());
                }
            },
            MENU_UPDATE => unsafe {
                if let Some(state) = state(hwnd)
                    && let Some(url) = state.update_url()
                    && let Err(err) = open_url(&url)
                {
                    show_error(hwnd, &err.to_string());
                }
            },
            MENU_QUIT => unsafe {
                DestroyWindow(hwnd);
            },
            _ => {}
        }
    }

    fn section_header_flags() -> u32 {
        MF_STRING | MF_GRAYED
    }

    fn checked_flag(checked: bool) -> u32 {
        if checked {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        }
    }

    fn update_menu_item(status: &CachedUpdateStatus) -> (String, u32) {
        match status {
            CachedUpdateStatus::Checking => ("Checking for updates...".to_string(), MF_GRAYED),
            CachedUpdateStatus::Latest => ("Already in latest version".to_string(), MF_STRING),
            CachedUpdateStatus::Available { version, .. } => {
                (format!("Update available ({version})"), MF_STRING)
            }
            CachedUpdateStatus::Failed => ("Unable to check updates".to_string(), MF_GRAYED),
        }
    }

    unsafe fn append_device_rows(menu: Hmenu) {
        match active_device_rows() {
            Ok(rows) if rows.is_empty() => {
                let label = wide("(none)");
                unsafe {
                    AppendMenuW(menu, section_header_flags(), 0, label.as_ptr());
                }
            }
            Ok(rows) => {
                for row in rows {
                    let label = wide(row);
                    unsafe {
                        AppendMenuW(menu, section_header_flags(), 0, label.as_ptr());
                    }
                }
            }
            Err(err) => {
                let label = wide(format!("Unable to list devices: {err}"));
                unsafe {
                    AppendMenuW(menu, section_header_flags(), 0, label.as_ptr());
                }
            }
        }
    }

    fn active_device_rows() -> Result<Vec<String>> {
        let platform = SystemDisplayPlatform::new();
        let count = platform.active_device_count()?;
        let mut rows = Vec::with_capacity(count);
        for index in 0..count {
            let name = platform.device_label(index)?;
            let mode = if platform.hdr_enabled(index)? {
                "HDR"
            } else {
                "SDR"
            };
            rows.push(format!("{index}: {name} ({mode})"));
        }

        Ok(rows)
    }

    fn default_settings_path() -> Result<PathBuf> {
        let exe = std::env::current_exe().map_err(|source| Error::Io { path: None, source })?;
        Ok(exe.parent().map_or_else(
            || PathBuf::from("settings.json"),
            |path| path.join("settings.json"),
        ))
    }

    fn configs_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("configs")))
            .unwrap_or_else(|| PathBuf::from("configs"))
    }

    fn preset_config_path(preset: &str) -> PathBuf {
        configs_dir().join(format!("{preset}.config.json"))
    }

    fn preset_items() -> Result<Vec<PresetItem>> {
        let mut presets = Vec::new();
        let directory = configs_dir();
        if !directory.is_dir() {
            return Ok(presets);
        }

        for entry in std::fs::read_dir(&directory).map_err(|source| Error::Io {
            path: Some(directory.clone()),
            source,
        })? {
            let entry = entry.map_err(|source| Error::Io {
                path: Some(directory.clone()),
                source,
            })?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(name) = file_name.strip_suffix(".config.json") else {
                continue;
            };

            presets.push(PresetItem {
                name: name.to_string(),
            });
        }

        presets.sort_by_key(|preset| preset.name.to_ascii_lowercase());
        Ok(presets)
    }

    fn resolve_existing_preset(current: &str) -> Result<String> {
        if preset_config_path(current).is_file() {
            return Ok(current.to_string());
        }

        let default = default_preset();
        if preset_config_path(&default).is_file() {
            return Ok(default);
        }

        Ok(preset_items()?
            .into_iter()
            .next()
            .map(|preset| preset.name)
            .unwrap_or_else(default_preset))
    }

    #[derive(Clone, Debug)]
    struct PresetItem {
        name: String,
    }

    #[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
    struct TraySettings {
        #[serde(default = "default_preset")]
        preset: String,
        #[serde(default = "default_true")]
        enabled: bool,
        #[serde(default = "default_true")]
        force: bool,
        #[serde(default)]
        start_with_windows: bool,
    }

    impl Default for TraySettings {
        fn default() -> Self {
            Self {
                preset: default_preset(),
                enabled: true,
                force: true,
                start_with_windows: false,
            }
        }
    }

    impl TraySettings {
        fn load(path: &std::path::Path) -> Result<Self> {
            if !path.is_file() {
                return Ok(Self::default());
            }

            let json = std::fs::read_to_string(path).map_err(|source| Error::Io {
                path: Some(path.to_path_buf()),
                source,
            })?;
            serde_json::from_str(&json).map_err(|source| {
                Error::InvalidArguments(format!("failed to parse {}: {source}", path.display()))
            })
        }

        fn save(&self, path: &std::path::Path) -> Result<()> {
            let json = serde_json::to_string_pretty(self).map_err(|source| {
                Error::InvalidArguments(format!("failed to serialize tray settings: {source}"))
            })?;
            std::fs::write(path, format!("{json}\n")).map_err(|source| Error::Io {
                path: Some(path.to_path_buf()),
                source,
            })
        }
    }

    fn default_preset() -> String {
        "default".to_string()
    }

    fn default_true() -> bool {
        true
    }

    fn open_in_explorer() -> Result<()> {
        let exe = std::env::current_exe().map_err(|source| Error::Io { path: None, source })?;
        let Some(parent) = exe.parent() else {
            return Err(Error::platform("could not find executable directory"));
        };

        open_path(parent)
    }

    fn open_config(path: &std::path::Path) -> Result<()> {
        if path.exists() {
            return open_path(path);
        }

        if let Some(parent) = path.parent() {
            return open_path(parent);
        }

        Err(Error::platform(format!(
            "configuration file does not exist: {}",
            path.display()
        )))
    }

    fn open_path(path: impl AsRef<std::path::Path>) -> Result<()> {
        open_shell_target(&path.as_ref().to_string_lossy())
    }

    fn open_url(url: &str) -> Result<()> {
        open_shell_target(url)
    }

    fn open_shell_target(target: &str) -> Result<()> {
        let operation = wide("open");
        let target = wide(target);
        let result = unsafe {
            ShellExecuteW(
                ptr::null_mut(),
                operation.as_ptr(),
                target.as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize;

        if result <= 32 {
            return Err(Error::platform(format!(
                "ShellExecuteW failed with status {result}"
            )));
        }

        Ok(())
    }

    unsafe fn handle_worker_done(hwnd: Hwnd) {
        if let Some(state) = unsafe { state(hwnd) }
            && let Err(err) = state.handle_worker_done()
        {
            unsafe {
                show_error(hwnd, &err.to_string());
            }
        }
    }

    unsafe fn cleanup(hwnd: Hwnd) {
        unsafe {
            remove_tray_icon(hwnd);
        }

        let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState };
        if state_ptr.is_null() {
            return;
        }

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        let mut state = unsafe { Box::from_raw(state_ptr) };
        let _ = state.stop_worker();
        if let Some(mut worker) = state.update_worker.take() {
            worker.stop();
        }
    }

    unsafe fn add_tray_icon(hwnd: Hwnd) -> Result<()> {
        let mut data = NotifyIconDataW {
            cb_size: size_of::<NotifyIconDataW>() as u32,
            hwnd,
            uid: TRAY_UID,
            uflags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            ucallback_message: WM_TRAY_ICON,
            hicon: unsafe { load_tray_icon() },
            ..NotifyIconDataW::default()
        };
        copy_wide(&mut data.sztip, APP_NAME);

        if unsafe { Shell_NotifyIconW(NIM_ADD, &mut data) } == 0 {
            return Err(last_os_error("Shell_NotifyIconW NIM_ADD failed"));
        }

        Ok(())
    }

    unsafe fn load_tray_icon() -> Hicon {
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            let icon_path = parent.join("icon.ico");
            if icon_path.is_file() {
                let icon_path = wide(icon_path.to_string_lossy());
                let icon = unsafe {
                    LoadImageW(
                        ptr::null_mut(),
                        icon_path.as_ptr(),
                        IMAGE_ICON,
                        0,
                        0,
                        LR_LOADFROMFILE | LR_DEFAULTSIZE,
                    )
                };
                if !icon.is_null() {
                    return icon;
                }
            }
        }

        unsafe { LoadIconW(ptr::null_mut(), IDI_APPLICATION as *const u16) }
    }

    unsafe fn remove_tray_icon(hwnd: Hwnd) {
        let mut data = NotifyIconDataW {
            cb_size: size_of::<NotifyIconDataW>() as u32,
            hwnd,
            uid: TRAY_UID,
            ..NotifyIconDataW::default()
        };
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &mut data);
        }
    }

    unsafe fn state<'a>(hwnd: Hwnd) -> Option<&'a mut TrayState> {
        let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState };
        if state_ptr.is_null() {
            None
        } else {
            Some(unsafe { &mut *state_ptr })
        }
    }

    unsafe fn show_error(hwnd: Hwnd, message: &str) {
        let title = wide(APP_NAME);
        let message = wide(message);
        unsafe {
            MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_ICONERROR);
        }
    }

    fn last_os_error(context: &str) -> Error {
        let error = std::io::Error::last_os_error();
        Error::platform(format!("{context}: {error}"))
    }

    fn wide(value: impl AsRef<str>) -> Vec<u16> {
        OsStr::new(value.as_ref())
            .encode_wide()
            .chain([0])
            .collect()
    }

    fn copy_wide(destination: &mut [u16], value: &str) {
        let value = wide(value);
        let len = value.len().min(destination.len());
        destination[..len].copy_from_slice(&value[..len]);
        if let Some(last) = destination.last_mut() {
            *last = 0;
        }
    }

    type Hwnd = *mut c_void;
    type Hmenu = *mut c_void;
    type Hinstance = *mut c_void;
    type Hicon = *mut c_void;
    type Hcursor = *mut c_void;
    type Hbrush = *mut c_void;
    type Handle = *mut c_void;

    struct SingleInstance {
        handle: Handle,
    }

    impl SingleInstance {
        fn acquire() -> Result<Option<Self>> {
            let name = wide(INSTANCE_MUTEX_NAME);
            let handle = unsafe { CreateMutexW(ptr::null_mut(), 1, name.as_ptr()) };
            if handle.is_null() {
                return Err(last_os_error("CreateMutexW failed"));
            }

            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe {
                    CloseHandle(handle);
                }
                return Ok(None);
            }

            Ok(Some(Self { handle }))
        }
    }

    impl Drop for SingleInstance {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    fn instance_running() -> bool {
        let Ok(Some(_instance)) = SingleInstance::acquire() else {
            return true;
        };
        false
    }

    #[repr(C)]
    struct WndClassExW {
        cb_size: u32,
        style: u32,
        lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: Hinstance,
        h_icon: Hicon,
        h_cursor: Hcursor,
        hbr_background: Hbrush,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
        h_icon_sm: Hicon,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NotifyIconDataW {
        cb_size: u32,
        hwnd: Hwnd,
        uid: u32,
        uflags: u32,
        ucallback_message: u32,
        hicon: Hicon,
        sztip: [u16; 128],
        dwstate: u32,
        dwstatemask: u32,
        szinfo: [u16; 256],
        timeout_or_version: u32,
        szinfotitle: [u16; 64],
        dwinfoflags: u32,
        guid_item: Guid,
        hballoonicon: Hicon,
    }

    impl Default for NotifyIconDataW {
        fn default() -> Self {
            Self {
                cb_size: 0,
                hwnd: ptr::null_mut(),
                uid: 0,
                uflags: 0,
                ucallback_message: 0,
                hicon: ptr::null_mut(),
                sztip: [0; 128],
                dwstate: 0,
                dwstatemask: 0,
                szinfo: [0; 256],
                timeout_or_version: 0,
                szinfotitle: [0; 64],
                dwinfoflags: 0,
                guid_item: Guid::default(),
                hballoonicon: ptr::null_mut(),
            }
        }
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassExW(class: *const WndClassExW) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: Hmenu,
            instance: Hinstance,
            param: *mut c_void,
        ) -> Hwnd;
        fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn PostQuitMessage(exit_code: i32);
        fn PostMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> i32;
        fn GetMessageW(message: *mut Msg, hwnd: Hwnd, min_filter: u32, max_filter: u32) -> i32;
        fn TranslateMessage(message: *const Msg) -> i32;
        fn DispatchMessageW(message: *const Msg) -> isize;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
        fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, flags: u32) -> i32;
        fn CreatePopupMenu() -> Hmenu;
        fn AppendMenuW(menu: Hmenu, flags: u32, new_item_id: usize, new_item: *const u16) -> i32;
        fn TrackPopupMenu(
            menu: Hmenu,
            flags: u32,
            x: i32,
            y: i32,
            reserved: i32,
            hwnd: Hwnd,
            rect: *const c_void,
        ) -> i32;
        fn DestroyMenu(menu: Hmenu) -> i32;
        fn GetCursorPos(point: *mut Point) -> i32;
        fn SetForegroundWindow(hwnd: Hwnd) -> i32;
        fn LoadIconW(instance: Hinstance, icon_name: *const u16) -> Hicon;
        fn LoadImageW(
            instance: Hinstance,
            name: *const u16,
            image_type: u32,
            desired_width: i32,
            desired_height: i32,
            load: u32,
        ) -> Hicon;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> Hinstance;
        fn CreateMutexW(attributes: *mut c_void, initial_owner: i32, name: *const u16) -> Handle;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn Shell_NotifyIconW(message: u32, data: *mut NotifyIconDataW) -> i32;
        fn ShellExecuteW(
            hwnd: Hwnd,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_command: i32,
        ) -> *mut c_void;
    }
}

#[cfg(windows)]
pub use windows_tray::launch;

#[cfg(windows)]
pub use windows_tray::run;

#[cfg(not(windows))]
pub fn launch(_config: Option<std::path::PathBuf>) -> crate::Result<()> {
    Err(crate::Error::platform(
        "system tray mode is only supported on Windows",
    ))
}

#[cfg(not(windows))]
pub fn run(_config: Option<std::path::PathBuf>) -> crate::Result<()> {
    Err(crate::Error::platform(
        "system tray mode is only supported on Windows",
    ))
}
