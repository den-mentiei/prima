#![allow(dead_code)]
#![allow(unused_variables)]

use std::error::Error;
use std::slice;
use std::str;

use ash::{vk, Entry};
use ash::extensions::khr;

fn main() -> Result<(), Box<dyn Error>> {
	println!("Hello, sailor!");
	unsafe { work() }
}

#[allow(non_snake_case)]
unsafe extern "stdcall" fn window_procedure(
	hwnd: ffi::HWND,
	uMsg: ffi::UINT,
	wParam: ffi::WPARAM,
	lParam: ffi::LPARAM,
) -> ffi::LRESULT {
	use ffi::*;

	match uMsg {
		WM_CLOSE   => drop(DestroyWindow(hwnd)),
		WM_DESTROY => PostQuitMessage(0),
		_ => return DefWindowProcA(hwnd, uMsg, wParam, lParam),
	};
	0
}

unsafe fn work() -> Result<(), Box<dyn Error>> {
	use core::ptr;
	use ffi::*;

	let hinstance = unsafe { GetModuleHandleA(ptr::null()) };

	let class_name = str_to_null_terminated_ascii("PRIMA_CLASS");

	let mut wc = WNDCLASSA::default();
	wc.lpfnWndProc = Some(window_procedure);
	wc.hInstance = hinstance;
	wc.lpszClassName = class_name.as_ptr();
	wc.hCursor = unsafe { LoadCursorA(ptr::null_mut(), IDC_ARROW) };

	let atom = unsafe { RegisterClassA(&wc) };
	if atom == 0 {
		let last_error = unsafe { GetLastError() };
		panic!("Failed to register the window class, error code = {last_error}");
	}

	let window_name = str_to_null_terminated_ascii("Prima!");
	let hwnd = unsafe {
		CreateWindowExA(
			0,
			class_name.as_ptr(),
			window_name.as_ptr(),
			WS_OVERLAPPEDWINDOW,
			CW_USEDEFAULT,
			CW_USEDEFAULT,
			CW_USEDEFAULT,
			CW_USEDEFAULT,
			ptr::null_mut(),
			ptr::null_mut(),
			hinstance,
			ptr::null_mut(),
		)
	};

	if hwnd.is_null() {
		panic!("Failed to create a window.");
	}

	let entry = Entry::load()?;

	// TODO: Enable validation layer (`VK_LAYER_KHRONOS_validation`).
	// and setup the debug callback to print messages.

	let app_info = vk::ApplicationInfo {
		api_version: vk::make_api_version(0, 1, 0, 0),
		..Default::default()
	};
	// VK_KHR_WIN32_SURFACE_EXTENSION_NAME
	// let create_info = vk::InstanceCreateInfo {
	// 	p_application_info: &app_info,
	// 	..Default::default()
	// };
	let extensions = [
		khr::Surface::name().as_ptr(),
		khr::Win32Surface::name().as_ptr(),
	];
	let create_info = vk::InstanceCreateInfo::builder()
		.application_info(&app_info)
		.enabled_extension_names(&extensions);

	let instance = entry.create_instance(&create_info, None)?;

	let surface_create_info = vk::Win32SurfaceCreateInfoKHR::builder()
		.hinstance(hinstance)
		.hwnd(hwnd);

	let khr_surface = khr::Surface::new(&entry, &instance);

	let khr_w32_surface = khr::Win32Surface::new(&entry, &instance);
	let surface = khr_w32_surface.create_win32_surface(&surface_create_info, None)?;

	let physical_devices = instance.enumerate_physical_devices()?;

	let mut physical_device  = None;
	let mut gfx_queue_family = None;
	for device in physical_devices.iter() {
		let queue_family_props = instance.get_physical_device_queue_family_properties(*device);
		let Some(queue_family) = queue_family_props.iter().position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS)) else {
			continue
		};

		let props = instance.get_physical_device_properties(*device);

		let name = str_from_null_terminated_bytes(&props.device_name);
		println!("Using the following device for graphics: {:?} ({:?})", name, props.device_type);

		physical_device  = Some(*device);
		gfx_queue_family = Some(queue_family as u32);
		break;
	}

	let Some(physical_device)  = physical_device else {
		panic!("No suitable physical device found.")
	};
	let Some(gfx_queue_family) = gfx_queue_family else {
		panic!("No graphics queue family found.")
	};

	let queue_priority    = [1.0];
	let queue_create_info = vk::DeviceQueueCreateInfo::builder()
		.queue_family_index(gfx_queue_family)
		.queue_priorities(&queue_priority);

	let device_create_info = vk::DeviceCreateInfo::builder()
		.queue_create_infos(slice::from_ref(&queue_create_info));

	let device = instance.create_device(physical_device, &device_create_info, None)?;

	let gfx_queue = device.get_device_queue(gfx_queue_family, 0);

	// TODO: Should be picked, while looping above.
	let can_present = khr_surface.get_physical_device_surface_support(physical_device, gfx_queue_family, surface)?;
	println!("Picked device/queue can present into the window's surface!");
	debug_assert!(can_present);

	unsafe { ShowWindow(hwnd, SW_SHOW) };

	let mut msg = MSG::default();
	loop {
		let got_msg = unsafe { GetMessageA(&mut msg, ptr::null_mut(), 0, 0) };
		match got_msg {
			0  => break,
			-1 => {
				let last_error = unsafe { GetLastError() };
				panic!("Failed to register the window class, error code = {last_error}");
			},
			_ => unsafe {
				TranslateMessage(&msg);
				DispatchMessageA(&msg);
			},
		}
	}

	unsafe {
		khr_surface.destroy_surface(surface, None);
		device.destroy_device(None);
		instance.destroy_instance(None);
	}

	Ok(())
}

unsafe fn str_from_null_terminated_bytes(bytes: &[i8]) -> &str {
	let n = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
	let s = slice::from_raw_parts(bytes.as_ptr() as *const u8, n);
	str::from_utf8_unchecked(s)
}

fn str_to_null_terminated_ascii(s: &str) -> Vec<i8> {
	debug_assert!(s.is_ascii());
	s.bytes().map(|b| b as i8).chain(Some(0)).collect()
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
mod ffi {
	use std::ffi;

	pub type WORD      = ffi::c_ushort;
	pub type DWORD     = ffi::c_ulong;
	pub type ATOM      = WORD;
	pub type BOOL      = ffi::c_int;
	pub type LPCSTR    = *const i8;
	pub type HINSTANCE = HANDLE;
	pub type HANDLE    = PVOID;
	pub type PVOID     = *mut ffi::c_void;
	pub type HICON     = HANDLE;
	pub type HBRUSH    = HANDLE;
	pub type HCURSOR   = HANDLE;
	pub type HMENU     = HANDLE;
	pub type LPVOID    = *mut ffi::c_void;
	pub type HWND      = HANDLE;
	pub type WPARAM    = UINT_PTR;
	pub type LPARAM    = LONG_PTR;
	pub type LRESULT   = LONG_PTR;
	pub type LONG      = ffi::c_long;
	pub type LONG_PTR  = isize;
	pub type ULONG_PTR = usize;
	pub type UINT      = ffi::c_uint;
	pub type UINT_PTR  = usize;

	pub const WS_OVERLAPPED: u32       = 0x00000000;
	pub const WS_CAPTION: u32          = 0x00C00000;
	pub const WS_SYSMENU: u32          = 0x00080000;
	pub const WS_THICKFRAME: u32       = 0x00040000;
	pub const WS_MINIMIZEBOX: u32      = 0x00020000;
	pub const WS_MAXIMIZEBOX: u32      = 0x00010000;
	pub const WS_OVERLAPPEDWINDOW: u32 = WS_OVERLAPPED
		| WS_CAPTION
		| WS_SYSMENU
		| WS_THICKFRAME
		| WS_MINIMIZEBOX
		| WS_MAXIMIZEBOX;
	pub const CW_USEDEFAULT: ffi::c_int = 0x80000000_u32 as ffi::c_int;

	pub const SW_SHOW: ffi::c_int = 5;

	pub const WM_CLOSE: u32   = 0x0010;
	pub const WM_DESTROY: u32 = 0x0002;

	pub const fn MAKEINTRESOURCEA(i: WORD) -> LPCSTR {
		i as ULONG_PTR as LPCSTR
	}

	pub const IDC_ARROW: LPCSTR = MAKEINTRESOURCEA(32512);

	pub type WNDPROC = Option<
		unsafe extern "stdcall" fn(
			hwnd: HWND,
			uMsg: UINT,
			wParam: WPARAM,
			lParam: LPARAM,
		) -> LRESULT,
	>;

	macro_rules! impl_zeroed_default {
		($name:ident) => (
			impl Default for $name {
				#[inline]
				#[must_use]
				fn default() -> Self {
					unsafe { core::mem::zeroed() }
				}
			}
		)
	}

	#[repr(C)]
	pub struct WNDCLASSA {
		pub style: UINT,
		pub lpfnWndProc: WNDPROC,
		pub cbClsExtra: ffi::c_int,
		pub cbWndExtra: ffi::c_int,
		pub hInstance: HINSTANCE,
		pub hIcon: HICON,
		pub hCursor: HCURSOR,
		pub hbrBackground: HBRUSH,
		pub lpszMenuName: LPCSTR,
		pub lpszClassName: LPCSTR,
	}

	impl_zeroed_default!(WNDCLASSA);

	#[repr(C)]
	pub struct POINT {
		x: LONG,
		y: LONG,
	}

	impl_zeroed_default!(POINT);

	#[repr(C)]
	pub struct MSG {
		hwnd: HWND,
		message: UINT,
		wParam: WPARAM,
		lParam: LPARAM,
		time: DWORD,
		pt: POINT,
		lPrivate: DWORD,
	}

	impl_zeroed_default!(MSG);

	#[link(name = "User32")]
	extern "stdcall" {
		pub fn RegisterClassA(
			lpWndClass: *const WNDCLASSA,
		) -> ATOM;

		pub fn UnregisterClassA(
			lpClassName: LPCSTR,
			hInstance: HINSTANCE,
		) -> BOOL;

		pub fn LoadCursorA(
			hInstance: HINSTANCE,
			lpCursorName: LPCSTR,
		) -> HCURSOR;

		pub fn CreateWindowExA(
			dwExStyle: DWORD,
			lpClassName: LPCSTR,
			lpWindowName: LPCSTR,
			dwStyle: DWORD,
			X: ffi::c_int,
			Y: ffi::c_int,
			nWidth: ffi::c_int,
			nHeight: ffi::c_int,
			hWndParent: HWND,
			hMenu: HMENU,
			hInstance: HINSTANCE,
			lpParam: LPVOID,
		) -> HWND;

		pub fn ShowWindow(
			hWnd: HWND,
			nCmdShow: ffi::c_int,
		) -> BOOL;

		pub fn DefWindowProcA(
			hWnd: HWND,
			Msg: UINT,
			wParam: WPARAM,
			lParam: LPARAM,
		) -> LRESULT;

		pub fn GetMessageA(
			lpMsg: *const MSG,
			hWnd: HWND,
			wMsgFilterMin: UINT,
			wMsgFilterMax: UINT,
		) -> BOOL;

		pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;

		pub fn DispatchMessageA(lpMsg: *const MSG) -> LRESULT;

		pub fn DestroyWindow(hWnd: HWND) -> BOOL;

		pub fn PostQuitMessage(nExitCode: ffi::c_int);
	}

	#[link(name = "Kernel32")]
	extern "stdcall" {
		pub fn GetLastError() -> DWORD;

		pub fn GetModuleHandleA(
			lpModuleName: LPCSTR,
		) -> HINSTANCE;
	}
}
