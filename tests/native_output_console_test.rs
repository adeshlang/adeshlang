//! Verify the UTF-8 to UTF-16 console writer at a chunk boundary.

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn supplementary_character_at_console_chunk_boundary_is_not_skipped() {
    use std::ffi::{CString, c_void};
    use std::sync::Mutex;

    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const CONSOLE_TEXTMODE_BUFFER: u32 = 1;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Coord {
        x: i16,
        y: i16,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AllocConsole() -> i32;
        fn CloseHandle(handle: isize) -> i32;
        fn CreateConsoleScreenBuffer(
            desired_access: u32,
            share_mode: u32,
            security_attributes: *const c_void,
            flags: u32,
            screen_buffer_data: *const c_void,
        ) -> isize;
        fn GetStdHandle(std_handle: u32) -> isize;
        fn ReadConsoleOutputCharacterW(
            console_output: isize,
            characters: *mut u16,
            length: u32,
            read_coord: Coord,
            chars_read: *mut u32,
        ) -> i32;
        fn SetConsoleCursorPosition(console_output: isize, cursor_position: Coord) -> i32;
        fn SetStdHandle(std_handle: u32, handle: isize) -> i32;
    }

    static STDOUT_LOCK: Mutex<()> = Mutex::new(());
    let _lock = STDOUT_LOCK.lock().expect("stdout test lock");

    let mut console = unsafe {
        CreateConsoleScreenBuffer(
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            CONSOLE_TEXTMODE_BUFFER,
            std::ptr::null(),
        )
    };
    let mut allocated_console = false;
    if console == 0 || console == -1 {
        if unsafe { AllocConsole() } != 0 {
            allocated_console = true;
        }
        console = unsafe {
            CreateConsoleScreenBuffer(
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                CONSOLE_TEXTMODE_BUFFER,
                std::ptr::null(),
            )
        };
    }
    assert!(
        console != 0 && console != -1,
        "create a test console buffer"
    );

    let original_stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    assert!(
        original_stdout != 0 && original_stdout != -1,
        "read original stdout handle"
    );
    assert_eq!(
        unsafe { SetConsoleCursorPosition(console, Coord { x: 0, y: 0 }) },
        1,
        "set test console cursor"
    );
    assert_eq!(
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, console) },
        1,
        "redirect stdout to test console buffer"
    );

    let text = CString::new(format!("{}😀Z", "x".repeat(255))).expect("NUL-free test text");
    unsafe {
        adesh_runtime::aot_print_cstr(text.as_ptr(), 0);
        assert_eq!(
            SetStdHandle(STD_OUTPUT_HANDLE, original_stdout),
            1,
            "restore process stdout"
        );
    }

    let mut cells = vec![0u16; 300];
    let mut chars_read = 0u32;
    assert_eq!(
        unsafe {
            ReadConsoleOutputCharacterW(
                console,
                cells.as_mut_ptr(),
                cells.len() as u32,
                Coord { x: 0, y: 0 },
                &mut chars_read,
            )
        },
        1,
        "read test console contents"
    );
    assert_eq!(
        unsafe { CloseHandle(console) },
        1,
        "close test console buffer"
    );
    if allocated_console {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn FreeConsole() -> i32;
        }
        let _ = unsafe { FreeConsole() };
    }

    let marker = cells[..chars_read as usize]
        .iter()
        .position(|cell| *cell == b'Z' as u16)
        .expect("marker after supplementary character should be present");
    assert!(
        marker >= 256,
        "the emoji boundary must not be skipped or rewound incorrectly, marker cell: {marker}"
    );
}
