#![allow(dead_code)]
#![allow(unused_variables)]

use std::error::Error;
use std::slice;
use std::str;

use ash::{vk, Entry};
use ash::extensions::khr;

const WIDTH:  u32 = 800;
const HEIGHT: u32 = 600;

// TODO: Handle window resizing.

fn main() -> Result<(), Box<dyn Error>> {
	println!("Hello, sailor!");
	unsafe { work() }
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
			// TODO: Center the window or load/save the last position.
			CW_USEDEFAULT,
			CW_USEDEFAULT,
			// TODO: Deal with scaling/HI-DPI & window title.
			WIDTH as i32,
			HEIGHT as i32,
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

	let (physical_device, queue_family) = physical_devices
		.iter()
		.find_map(|pdevice| {
			instance
				.get_physical_device_queue_family_properties(*pdevice)
				.iter()
				.enumerate()
				.find_map(|(i, props)| {
					let has_gfx     = props.queue_flags.contains(vk::QueueFlags::GRAPHICS);
					let can_present = khr_surface.get_physical_device_surface_support(*pdevice, i as u32, surface).ok()?;
					if has_gfx && can_present {
						Some((*pdevice, i as u32))
					} else {
						None
					}
				})
		})
		.expect("Failed to find a suitable physical device.");

	let props = instance.get_physical_device_properties(physical_device);
	let name  = str_from_null_terminated_bytes(&props.device_name);
	println!("Using the following physical device: {:?} ({:?})", name, props.device_type);

	let queue_priority = [1.0];

	let queue_create_info = vk::DeviceQueueCreateInfo::builder()
		.queue_family_index(queue_family)
		.queue_priorities(&queue_priority);

	let device_extensions = [
		khr::Swapchain::name().as_ptr(),
	];
	let device_create_info = vk::DeviceCreateInfo::builder()
		.queue_create_infos(slice::from_ref(&queue_create_info))
		.enabled_extension_names(&device_extensions);

	let device = instance.create_device(physical_device, &device_create_info, None)?;
	let queue  = device.get_device_queue(queue_family, 0);

	let surface_formats = khr_surface.get_physical_device_surface_formats(physical_device, surface)?;
	println!("Swapchain supported swapchain formats: {:#?}", surface_formats);
	let surface_format = surface_formats
		.iter()
		.find(|f| f.format == vk::Format::B8G8R8A8_UNORM && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
		.expect("Swapchain doesn't support the srgb_bgra8.");
	println!("Swapchain format: {:?}", surface_format);

	let present_modes = khr_surface.get_physical_device_surface_present_modes(physical_device, surface)?;
	println!("Physical device supported swapchain presentation modes: {:#?}", present_modes);
	// let present_mode = present_modes
	// 	.iter()
	// 	.copied()
	// 	.find(|m| *m == vk::PresentModeKHR::MAILBOX) // more energy but lowest latency
	// 	.unwrap_or(vk::PresentModeKHR::FIFO); // guaranteed to be available.
	let present_mode = vk::PresentModeKHR::FIFO;
	println!("Presentation mode: {:?}", present_mode);

	let surface_caps = khr_surface.get_physical_device_surface_capabilities(physical_device, surface)?;
	println!("{:#?}", surface_caps);

	let mut client_rect = RECT::default();
	let err = unsafe { GetClientRect(hwnd, &mut client_rect) };
	if err != 1 {
		panic!("Failed to get window client rect.");
	}
	let window_client_size = (
		(client_rect.right - client_rect.left) as u32,
		(client_rect.bottom - client_rect.top) as u32,
	);
	println!("Window client size: {:?}", window_client_size);

	let swapchain_size = (
		window_client_size.0.clamp(surface_caps.min_image_extent.width,  surface_caps.min_image_extent.width),
		window_client_size.1.clamp(surface_caps.min_image_extent.height, surface_caps.min_image_extent.height),
	);
	println!("Swapchain size: {:?}", swapchain_size);
	let swapchain_extent = vk::Extent2D::builder()
		.width(swapchain_size.0)
		.height(swapchain_size.1);

	let swapchain_image_count = surface_caps.min_image_count;
	println!("Swapchain image count: {swapchain_image_count}");

	let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
		.surface(surface)
		.min_image_count(swapchain_image_count)
		.image_format(surface_format.format)
		.image_color_space(surface_format.color_space)
		.image_extent(*swapchain_extent)
		.image_array_layers(1)
		.image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
		.image_sharing_mode(vk::SharingMode::EXCLUSIVE) // we have same queue for graphics & presentation
		.pre_transform(surface_caps.current_transform)
		.composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
		.present_mode(present_mode)
		.clipped(true);

	let khr_swapchain = khr::Swapchain::new(&instance, &device);
	let swapchain = khr_swapchain.create_swapchain(&swapchain_create_info, None)?;

	let swapchain_images = khr_swapchain.get_swapchain_images(swapchain)?;
	assert!(swapchain_images.len() == swapchain_image_count as usize);

	let mut swapchain_image_views = Vec::with_capacity(swapchain_images.len());
	for image in swapchain_images.iter() {
		let image_view_create_info = vk::ImageViewCreateInfo::builder()
			.view_type(vk::ImageViewType::TYPE_2D)
			.format(surface_format.format)
			.components(vk::ComponentMapping {
				r: vk::ComponentSwizzle::R,
				g: vk::ComponentSwizzle::G,
				b: vk::ComponentSwizzle::B,
				a: vk::ComponentSwizzle::A,
			})
			.subresource_range(vk::ImageSubresourceRange {
				aspect_mask: vk::ImageAspectFlags::COLOR,
				base_mip_level: 0,
				level_count: 0,
				base_array_layer: 0,
				layer_count: 1,
			})
			.image(*image);
		let image_view = device.create_image_view(&image_view_create_info, None)?;
		swapchain_image_views.push(image_view);
	}

	let mut swapchain_framebuffers = Vec::with_capacity(swapchain_image_views.len());
	for image_view in swapchain_image_views.iter() {
		let image_views = [*image_view];
		let framebuffer_create_info = vk::FramebufferCreateInfo::builder()
			.attachments(&image_views[..])
			.width(swapchain_extent.width)
			.height(swapchain_extent.height)
			.layers(1);
		let framebuffer = device.create_framebuffer(&framebuffer_create_info, None)?;
		swapchain_framebuffers.push(framebuffer);
	}
	// @Incomplete Specify render pass.
	// @Incomplete Destroy framebuffers.

	let semaphore_create_info = vk::SemaphoreCreateInfo::default();
	let acquire_semaphore     = device.create_semaphore(&semaphore_create_info, None)?;
	let release_semaphore     = device.create_semaphore(&semaphore_create_info, None)?;

	let pool_create_info = vk::CommandPoolCreateInfo::builder()
		.queue_family_index(queue_family)
		.flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
	let command_pool = device.create_command_pool(&pool_create_info, None)?;

	let cmd_buffer_alloc_info = vk::CommandBufferAllocateInfo::builder()
		.command_buffer_count(1)
		.command_pool(command_pool)
		.level(vk::CommandBufferLevel::PRIMARY);

	let cmd_buffers = device.allocate_command_buffers(&cmd_buffer_alloc_info)?;
	assert!(!cmd_buffers.is_empty());
	let cmd_buffer = cmd_buffers[0];

	let fence_create_info = vk::FenceCreateInfo::builder()
		.flags(vk::FenceCreateFlags::SIGNALED);

	let draw_reuse_fence = device.create_fence(&fence_create_info, None)?;

	// TODO: Don't be so crude with the timeout.
	// device.wait_for_fences(&[draw_reuse_fence], true, u64::MAX)?;
	// device.reset_fences(&[draw_reuse_fence])?;

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

		let (i, _is_suboptimal) = khr_swapchain.acquire_next_image(swapchain, u64::MAX, acquire_semaphore, vk::Fence::null())?;

		device.reset_command_buffer(cmd_buffer, vk::CommandBufferResetFlags::RELEASE_RESOURCES)?;

		let cmd_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
			.flags(vk::CommandBufferUsageFlags::empty());
		device.begin_command_buffer(cmd_buffer, &cmd_buffer_begin_info)?;

		let mut clear_color = vk::ClearColorValue::default();
		clear_color.float32 = [1.0, 0.0, 1.0, 1.0];

		let range = vk::ImageSubresourceRange {
			aspect_mask: vk::ImageAspectFlags::COLOR,
			base_mip_level: 0,
			level_count: 1,
			base_array_layer: 0,
			layer_count: 1,
		};
		let image = swapchain_images[i as usize];
		device.cmd_clear_color_image(cmd_buffer, image, vk::ImageLayout::GENERAL, &clear_color, slice::from_ref(&range));

		device.end_command_buffer(cmd_buffer)?;

		let submit_stage_mask = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
		let submit_info = vk::SubmitInfo::builder()
			.wait_semaphores(slice::from_ref(&acquire_semaphore))
			.signal_semaphores(slice::from_ref(&release_semaphore))
			.wait_dst_stage_mask(slice::from_ref(&submit_stage_mask))
			.command_buffers(slice::from_ref(&cmd_buffer));
		device.queue_submit(queue, slice::from_ref(&*submit_info), vk::Fence::null())?;

		let present_info  = vk::PresentInfoKHR::builder()
			.wait_semaphores(slice::from_ref(&release_semaphore))
			.swapchains(slice::from_ref(&swapchain))
			.image_indices(slice::from_ref(&i));
		khr_swapchain.queue_present(queue, &present_info)?;
		device.device_wait_idle()?;
	}

	// TODO: @bug Do a drop/defer guards for this.
	unsafe {
		device.device_wait_idle()?;

		device.destroy_fence(draw_reuse_fence, None);
		device.destroy_command_pool(command_pool, None);
		device.destroy_semaphore(release_semaphore, None);
		device.destroy_semaphore(acquire_semaphore, None);
		for image_view in swapchain_image_views {
			device.destroy_image_view(image_view, None);
		}
		khr_swapchain.destroy_swapchain(swapchain, None);
		khr_surface.destroy_surface(surface, None);
		device.destroy_device(None);
		instance.destroy_instance(None);
	}

	Ok(())
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
	pub type LPRECT    = *mut RECT;

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
		pub x: LONG,
		pub y: LONG,
	}

	impl_zeroed_default!(POINT);

	#[repr(C)]
	pub struct MSG {
		pub hwnd: HWND,
		pub message: UINT,
		pub wParam: WPARAM,
		pub lParam: LPARAM,
		pub time: DWORD,
		pub pt: POINT,
		pub lPrivate: DWORD,
	}

	impl_zeroed_default!(MSG);

	#[repr(C)]
	pub struct RECT {
		pub left: LONG,
		pub top: LONG,
		pub right: LONG,
		pub bottom: LONG,
	}

	impl_zeroed_default!(RECT);

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

		pub fn GetClientRect(hwnd: HWND, lpRect: LPRECT) -> BOOL;
	}

	#[link(name = "Kernel32")]
	extern "stdcall" {
		pub fn GetLastError() -> DWORD;

		pub fn GetModuleHandleA(
			lpModuleName: LPCSTR,
		) -> HINSTANCE;
	}
}
