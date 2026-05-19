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
    use std::sync::{Arc, mpsc};
    use std::thread::{self, JoinHandle};

    use crate::app::{self, RuntimeOptions, TweakOptions};
    use crate::error::{Error, Result};
    use crate::platform::SystemDisplayPlatform;

    const APP_NAME: &str = "Color LUT Tweaks";
    const WM_APP: u32 = 0x8000;
    const WM_TRAY_ICON: u32 = WM_APP + 1;
    const WM_WORKER_DONE: u32 = WM_APP + 2;
    const WM_DESTROY: u32 = 0x0002;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const GWLP_USERDATA: i32 = -21;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

    const NIM_ADD: u32 = 0x00000000;
    const NIM_DELETE: u32 = 0x00000002;
    const NIF_MESSAGE: u32 = 0x00000001;
    const NIF_ICON: u32 = 0x00000002;
    const NIF_TIP: u32 = 0x00000004;

    const MF_STRING: u32 = 0x00000000;
    const MF_CHECKED: u32 = 0x00000008;
    const MF_SEPARATOR: u32 = 0x00000800;
    const TPM_RETURNCMD: u32 = 0x00000100;
    const TPM_RIGHTBUTTON: u32 = 0x00000002;

    const MB_ICONERROR: u32 = 0x00000010;
    const IMAGE_ICON: u32 = 1;
    const IDI_APPLICATION: usize = 32512;
    const LR_LOADFROMFILE: u32 = 0x00000010;
    const LR_DEFAULTSIZE: u32 = 0x00000040;

    const TRAY_UID: u32 = 1;
    const MENU_ENABLED: usize = 1001;
    const MENU_FORCE: usize = 1002;
    const MENU_RELOAD: usize = 1003;
    const MENU_QUIT: usize = 1004;

    pub fn launch(config: Option<PathBuf>) -> Result<()> {
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
        let config = config.unwrap_or(app::default_config_path()?);

        unsafe {
            let hwnd = create_window()?;
            let mut state = Box::new(TrayState {
                enabled: true,
                force: true,
                worker: None,
                config,
            });
            state.start_worker(hwnd)?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
            add_tray_icon(hwnd)?;
            message_loop()?;
        }

        Ok(())
    }

    struct TrayState {
        enabled: bool,
        force: bool,
        worker: Option<RuntimeWorker>,
        config: PathBuf,
    }

    struct RuntimeWorker {
        shutdown: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
        result: mpsc::Receiver<Result<()>>,
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

            Ok(())
        }

        fn toggle_force(&mut self, hwnd: Hwnd) -> Result<()> {
            self.force = !self.force;
            if self.enabled {
                self.reload(hwnd)?;
            }

            Ok(())
        }

        fn reload(&mut self, hwnd: Hwnd) -> Result<()> {
            self.stop_worker()?;
            if self.enabled {
                if let Err(err) = self.start_worker(hwnd) {
                    self.enabled = false;
                    return Err(err);
                }
            }

            Ok(())
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
            WM_TRAY_ICON => {
                if lparam as u32 == WM_RBUTTONUP
                    || lparam as u32 == WM_LBUTTONUP
                    || lparam as u32 == WM_CONTEXTMENU
                {
                    unsafe {
                        show_menu(hwnd);
                    }
                    return 0;
                }
            }
            WM_WORKER_DONE => unsafe {
                handle_worker_done(hwnd);
                return 0;
            },
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

        let enabled = wide("Enabled");
        let force = wide("Force");
        let reload = wide("Reload");
        let quit = wide("Quit");
        let (enabled_flags, force_flags) = if let Some(state) = unsafe { state(hwnd) } {
            (checked_flag(state.enabled), checked_flag(state.force))
        } else {
            (MF_STRING, MF_STRING)
        };
        unsafe {
            AppendMenuW(menu, enabled_flags, MENU_ENABLED, enabled.as_ptr());
            AppendMenuW(menu, force_flags, MENU_FORCE, force.as_ptr());
            AppendMenuW(menu, MF_STRING, MENU_RELOAD, reload.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
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

        match command as usize {
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
            MENU_RELOAD => unsafe {
                if let Some(state) = state(hwnd)
                    && let Err(err) = state.reload(hwnd)
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

    fn checked_flag(checked: bool) -> u32 {
        if checked {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        }
    }

    unsafe fn handle_worker_done(hwnd: Hwnd) {
        if let Some(state) = unsafe { state(hwnd) } {
            if let Err(err) = state.handle_worker_done() {
                unsafe {
                    show_error(hwnd, &err.to_string());
                }
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
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn Shell_NotifyIconW(message: u32, data: *mut NotifyIconDataW) -> i32;
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
