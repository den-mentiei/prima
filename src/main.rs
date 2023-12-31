#![allow(dead_code)]
#![allow(unused_variables)]

use std::error::Error;
use std::ffi::{c_void, CStr};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::ptr;
use std::slice;
use std::str;

use ash::{vk, Entry};
use ash::extensions::{ext, khr};

use ffi::*;

const WIDTH:  u32 = 800;
const HEIGHT: u32 = 600;

const WINDOW_CLASS_NAME: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"PRIMA_CLASS\0") };

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// TODO: Handle window resizing.

fn main() -> Result<()> {
	println!("Hello, sailor!");
	unsafe { work() }
}

unsafe fn work() -> Result<()> {
	let hinstance = GetModuleHandleA(ptr::null());

	let atom = register_window_class(hinstance);
	let hwnd = create_window(hinstance, WIDTH, HEIGHT);

	let entry    = Entry::load()?;
	let instance = create_instance(&entry)?;

	let debug_utils = ext::DebugUtils::new(&entry, &instance);

	let dbg_messenger_create_info = vk::DebugUtilsMessengerCreateInfoEXT::default();
	// TODO: Make a nice callback, then enable this back.
	// let dbg_messenger_create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
	// 	.pfn_user_callback(Some(vulkan_debug_message_callback));
	// let dbg_messenger = debug_utils.create_debug_utils_messenger(&dbg_messenger_create_info, None)?;

	let surface_create_info = vk::Win32SurfaceCreateInfoKHR::builder()
		.hinstance(hinstance)
		.hwnd(hwnd);

	let khr_surface     = khr::Surface::new(&entry, &instance);
	let khr_w32_surface = khr::Win32Surface::new(&entry, &instance);
	let surface         = khr_w32_surface.create_win32_surface(&surface_create_info, None)?;

	let (physical_device, queue_family) = pick_physical_device_and_queue_family(
		&instance,
		&khr_surface,
		surface,
	)?;

	let props = instance.get_physical_device_properties(physical_device);
	let name  = str_from_null_terminated_bytes(&props.device_name);
	println!("Using the following physical device: {:?} ({:?})", name, props.device_type);

	let (device, queue) = create_device_and_queue(
		&instance,
		physical_device,
		queue_family,
	)?;

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

	let window_client_size = get_window_client_size(hwnd);
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
				level_count: 1,
				base_array_layer: 0,
				layer_count: 1,
			})
			.image(*image);
		let image_view = device.create_image_view(&image_view_create_info, None)?;
		swapchain_image_views.push(image_view);
	}

	let attachment = vk::AttachmentDescription::builder()
		.format(surface_format.format)
		.samples(vk::SampleCountFlags::TYPE_1)
		.load_op(vk::AttachmentLoadOp::CLEAR)
		.store_op(vk::AttachmentStoreOp::STORE)
		.stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
		.stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
		.initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
		.final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

	let attachment_ref = vk::AttachmentReference::builder()
		.attachment(0)
		.layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
	let subpass = vk::SubpassDescription::builder()
		.pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
		.color_attachments(slice::from_ref(&attachment_ref));

	let render_pass_create_info = vk::RenderPassCreateInfo::builder()
		.subpasses(slice::from_ref(&subpass))
		.attachments(slice::from_ref(&attachment));
	let render_pass = device.create_render_pass(&render_pass_create_info, None)?;

	let mut swapchain_framebuffers = Vec::with_capacity(swapchain_image_views.len());
	for image_view in swapchain_image_views.iter() {
		let framebuffer_create_info = vk::FramebufferCreateInfo::builder()
			.render_pass(render_pass)
			.attachments(slice::from_ref(image_view))
			.width(swapchain_extent.width)
			.height(swapchain_extent.height)
			.layers(1);
		let framebuffer = device.create_framebuffer(&framebuffer_create_info, None)?;
		swapchain_framebuffers.push(framebuffer);
	}

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

	let descriptor_set_layout_binding = vk::DescriptorSetLayoutBinding::builder()
		.binding(0)
		.descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
		.descriptor_count(1)
		.stage_flags(vk::ShaderStageFlags::VERTEX);

	let descriptor_set_layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
		.bindings(slice::from_ref(&descriptor_set_layout_binding));
	let descriptor_set_layout = device.create_descriptor_set_layout(&descriptor_set_layout_create_info, None)?;

	let max_frames_in_flight = 2;

	let descriptor_sizes = [
		vk::DescriptorPoolSize {
			ty: vk::DescriptorType::STORAGE_BUFFER,
			descriptor_count: max_frames_in_flight,
		},
	];
	let descriptor_pool_create_info = vk::DescriptorPoolCreateInfo::builder()
		.max_sets(max_frames_in_flight)
		.pool_sizes(&descriptor_sizes);
	let descriptor_pool = device.create_descriptor_pool(&descriptor_pool_create_info, None)?;

	let set_layouts = vec![descriptor_set_layout; max_frames_in_flight as usize];
	let descriptor_set_alloc_info = vk::DescriptorSetAllocateInfo::builder()
		.descriptor_pool(descriptor_pool)
		.set_layouts(&set_layouts);
	let descriptor_sets = device.allocate_descriptor_sets(&descriptor_set_alloc_info)?;

	let prima_size_per_frame = 64 * 1024;

	let pbuffer_create_info = vk::BufferCreateInfo::builder()
		.size(prima_size_per_frame * max_frames_in_flight as u64)
		.usage(vk::BufferUsageFlags::STORAGE_BUFFER)
		.sharing_mode(vk::SharingMode::EXCLUSIVE);
	let pbuffer = device.create_buffer(&pbuffer_create_info, None)?;

	let ibuffer_create_info = vk::BufferCreateInfo::builder()
		.size(prima_size_per_frame * max_frames_in_flight as u64)
		.usage(vk::BufferUsageFlags::INDEX_BUFFER)
		.sharing_mode(vk::SharingMode::EXCLUSIVE);
	let ibuffer = device.create_buffer(&ibuffer_create_info, None)?;
	// @Incomplete Need to check the requirements, as well.
	// Sub-allocate, etc.

	let mem_req       = device.get_buffer_memory_requirements(pbuffer);
	let mem_props     = instance.get_physical_device_memory_properties(physical_device);
	let mut mem_index = None;
	let host_coherent = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
	for i in 0..mem_props.memory_type_count {
		let mem_type_is_fine   = mem_req.memory_type_bits & (1 << i) != 0;
		let mem_type_flags     = mem_props.memory_types[i as usize].property_flags;
		let mem_flags_are_fine = mem_type_flags.contains(host_coherent);
		if mem_type_is_fine && mem_flags_are_fine {
			mem_index = Some(i);
		}
	}
	let Some(mem_index) = mem_index else {
		panic!("Failed to find a suitable SSBO memory.");
	};

	let total_memory   = pbuffer_create_info.size + ibuffer_create_info.size;
	let mem_alloc_info = vk::MemoryAllocateInfo::builder()
		.allocation_size(total_memory)
		.memory_type_index(mem_index);
	let buffer_mem = device.allocate_memory(&mem_alloc_info, None)?;

	device.bind_buffer_memory(pbuffer, buffer_mem, 0)?;
	device.bind_buffer_memory(ibuffer, buffer_mem, pbuffer_create_info.size)?;

	let buffer_ptr  = device.map_memory(buffer_mem, 0, total_memory, vk::MemoryMapFlags::default())?;
	let buffer_data = slice::from_raw_parts_mut(buffer_ptr as *mut u8, total_memory as usize);

	let pbuffer_size = pbuffer_create_info.size as usize;

	let (pbuffers, ibuffers) = buffer_data.split_at_mut(pbuffer_size);

	let (pb0, pb1) = pbuffers.split_at_mut(prima_size_per_frame as usize);
	let pbuffers   = [pb0, pb1];

	let (ib0, ib1) = ibuffers.split_at_mut(prima_size_per_frame as usize);
	let ibuffers   = [ib0, ib1];

	for (i, set) in descriptor_sets.iter().enumerate() {
		let buffer_info = vk::DescriptorBufferInfo::builder()
			.buffer(pbuffer)
			.offset(i as u64 * prima_size_per_frame)
			.range(prima_size_per_frame);
		let descriptor_write = vk::WriteDescriptorSet::builder()
			.dst_set(*set)
			.dst_binding(0)
			.dst_array_element(0)
			.descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
			.buffer_info(slice::from_ref(&buffer_info));
		device.update_descriptor_sets(slice::from_ref(&descriptor_write), &[]);
	}

	let tri_pipeline = create_tri_pipeline(&device, render_pass, descriptor_set_layout)?;

	unsafe { ShowWindow(hwnd, SW_SHOW) };

	let mut msg = MSG::default();
	loop {
		// Lazy drawing.
		// let got_msg = unsafe { GetMessageA(&mut msg, ptr::null_mut(), 0, 0) };
		// The usual v-synced drawing.
		let got_msg = unsafe { PeekMessageA(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) };
		match got_msg {
			0  => (), // Execute the rendering code below.
			-1 => {
				let last_error = unsafe { GetLastError() };
				panic!("Failed to peek window messages, error code = {last_error}");
			},
			_ => unsafe {
				TranslateMessage(&msg);
				DispatchMessageA(&msg);

				if msg.message == WM_QUIT {
					break;
				}
			},
		}

		let next_image = khr_swapchain.acquire_next_image(swapchain, u64::MAX, acquire_semaphore, vk::Fence::null());
		let i = match next_image {
			Ok((i, _)) => i,
			Err(e) => {
				eprintln!("Failed to get the next swapchain image due to {:?}", e);
				break;
			},
		};

		// Preparation

		// @Incomplete Cleanup and make the double-buffered scheme more elegant.
		// There is no need to acquire an image to know which index to write.

		let w = swapchain_extent.width  as f32;
		let h = swapchain_extent.height as f32;

		let pbuf    = pbuffers[i as usize].as_mut_ptr() as *mut f32;
		let ibuf    = ibuffers[i as usize].as_mut_ptr() as *mut u32;
		let indices = fill_prima_buffers(w, h, pbuf, ibuf);

		// Rendering

		device.reset_command_pool(command_pool, vk::CommandPoolResetFlags::empty())?;

		let cmd_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
			.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
		device.begin_command_buffer(cmd_buffer, &cmd_buffer_begin_info)?;

		let image = swapchain_images[i as usize];
		let render_begin_barrier = image_barrier(
			&image,
			vk::AccessFlags::empty(),
			vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
			vk::ImageLayout::UNDEFINED,
			vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
		);
		device.cmd_pipeline_barrier(
			cmd_buffer,
			vk::PipelineStageFlags::BOTTOM_OF_PIPE,
			vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
			vk::DependencyFlags::BY_REGION,
			&[],
			&[],
			slice::from_ref(&render_begin_barrier),
		);

		let mut clear_color = vk::ClearColorValue::default();
		clear_color.float32 = [0.4, 0.6, 0.45, 1.0];

		let mut clear_value = vk::ClearValue::default();
		clear_value.color   = clear_color;

		let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
			.render_pass(render_pass)
			.framebuffer(swapchain_framebuffers[i as usize])
			.render_area(vk::Rect2D {
				offset: vk::Offset2D::default(),
				extent: *swapchain_extent,
			})
			.clear_values(slice::from_ref(&clear_value));

		device.cmd_begin_render_pass(cmd_buffer, &render_pass_begin_info, vk::SubpassContents::INLINE);

		device.cmd_bind_descriptor_sets(
			cmd_buffer,
			vk::PipelineBindPoint::GRAPHICS,
			tri_pipeline.layout,
			0,
			slice::from_ref(&descriptor_sets[i as usize]),
			&[]);
		let ibuffer_offset = i as u64 * prima_size_per_frame;
		device.cmd_bind_index_buffer(cmd_buffer, ibuffer, ibuffer_offset, vk::IndexType::UINT32);

		let viewport = vk::Viewport {
			x: 0.0,
			y: 0.0,
			width:  swapchain_extent.width  as f32,
			height: swapchain_extent.height as f32,
			min_depth: 0.0,
			max_depth: 0.1,
		};
		let scissor = vk::Rect2D {
			offset: vk::Offset2D::default(),
			extent: *swapchain_extent,
		};
		device.cmd_set_viewport(cmd_buffer, 0, slice::from_ref(&viewport));
		device.cmd_set_scissor(cmd_buffer, 0, slice::from_ref(&scissor));
		device.cmd_bind_pipeline(cmd_buffer, vk::PipelineBindPoint::GRAPHICS, tri_pipeline.handle);
		device.cmd_draw_indexed(cmd_buffer, indices as u32, 1, 0, 0, 0);

		device.cmd_end_render_pass(cmd_buffer);

		let render_end_barrier = image_barrier(
			&image,
			vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
			vk::AccessFlags::empty(),
			vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
			vk::ImageLayout::PRESENT_SRC_KHR,
		);
		device.cmd_pipeline_barrier(
			cmd_buffer,
			vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
			vk::PipelineStageFlags::TOP_OF_PIPE,
			vk::DependencyFlags::BY_REGION,
			&[],
			&[],
			slice::from_ref(&render_end_barrier),
		);

		device.end_command_buffer(cmd_buffer)?;

		let submit_stage_mask = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
		let submit_info       = vk::SubmitInfo::builder()
			.wait_semaphores(slice::from_ref(&acquire_semaphore))
			.signal_semaphores(slice::from_ref(&release_semaphore))
			.wait_dst_stage_mask(slice::from_ref(&submit_stage_mask))
			.command_buffers(slice::from_ref(&cmd_buffer));
		device.queue_submit(queue, slice::from_ref(&*submit_info), vk::Fence::null())?;

		let present_info = vk::PresentInfoKHR::builder()
			.wait_semaphores(slice::from_ref(&release_semaphore))
			.swapchains(slice::from_ref(&swapchain))
			.image_indices(slice::from_ref(&i));
		let result = khr_swapchain.queue_present(queue, &present_info);
		if result.is_err() {
			eprintln!("Failed to present due to {:?}", result);
			break;
		}
		device.device_wait_idle()?;
	}

	// TODO: @bug Do a drop/defer guards for this.
	unsafe {
		device.device_wait_idle()?;

		device.destroy_pipeline(tri_pipeline.handle, None);
		device.destroy_pipeline_layout(tri_pipeline.layout, None);
		device.destroy_buffer(ibuffer, None);
		device.destroy_buffer(pbuffer, None);
		device.destroy_descriptor_pool(descriptor_pool, None);
		device.unmap_memory(buffer_mem);
		device.free_memory(buffer_mem, None);
		device.destroy_descriptor_set_layout(descriptor_set_layout, None);
		device.destroy_command_pool(command_pool, None);
		device.destroy_semaphore(release_semaphore, None);
		device.destroy_semaphore(acquire_semaphore, None);
		for framebuffer in swapchain_framebuffers {
			device.destroy_framebuffer(framebuffer, None);
		}
		device.destroy_render_pass(render_pass, None);
		for image_view in swapchain_image_views {
			device.destroy_image_view(image_view, None);
		}
		khr_swapchain.destroy_swapchain(swapchain, None);
		khr_surface.destroy_surface(surface, None);
		device.destroy_device(None);
		// debug_utils.destroy_debug_utils_messenger(dbg_messenger, None);
		instance.destroy_instance(None);
	}

	println!("Kthnx, bye!");

	Ok(())
}

//
// Index is encoded as follows:
//
// [31:27] [26:25] [24:0]
//    |       |       |
//    |       |       +------- offset info prima buffer
//    |       +--------------- rect corner id
//    +----------------------- primitive type
//
// Prima buffer at offset will contain primitive type
// specific data.
//
// Supported primitive types & their data:
//
// * PRIMA_TRIANGLE:
//
//   Buffer data:
//
//   struct TriVertex {
//     x: f32,
//     y: f32,
//   };
//
//   Indices: (0, 1, 2)
//
// * PRIMA_RECT:
//
//   Buffer data:
//
//   struct Rect {
//     x: f32,
//     y: f32,
//     w: f32,
//     h: f32,
//   };
//
//   Indices:
//
//     1 +--+ 2
//       | /|
//       |/ |
//     0 +__+ 3
//
//     (0, 1, 2, 2, 3, 0)
//
unsafe fn fill_prima_buffers(w: f32, h: f32, p: *mut f32, i: *mut u32) -> usize {
	let mut indices = 0;
	let mut offset  = 0; // First ever primitive.

	let proj = ortho_projection(w, h);
	p.copy_from(proj.as_ptr() as *const f32, 16);
	let p = p.add(16);
	offset += 16;

	const PRIMA_TRI:  u32 = 0;
	const PRIMA_RECT: u32 = 1;

	const fn make_index(offset: u32, p_type: u32, corner: u8) -> u32 {
		(p_type << 26) | ((corner as u32) << 24) | (offset as u32)
	}

	// Rect.

	let p_type = PRIMA_RECT;

	let rect_indices = [
		make_index(offset, p_type, 0),
		make_index(offset, p_type, 1),
		make_index(offset, p_type, 2),
		make_index(offset, p_type, 2),
		make_index(offset, p_type, 3),
		make_index(offset, p_type, 0),
	];

	p.add(0).write_unaligned(50.0);  // x
	p.add(1).write_unaligned(150.0); // y
	p.add(2).write_unaligned(200.0); // w
	p.add(3).write_unaligned(120.0); // h
	let p = p.add(4);
	offset += 4;

	for (offset, index) in rect_indices.into_iter().enumerate() {
		i.add(offset).write_unaligned(index);
		indices += 1;
	}
	let i = i.add(indices);

	// Triangle

	let p_type = PRIMA_TRI;

	let tri_indices = [
		make_index(offset,     p_type, 0),
		make_index(offset + 3, p_type, 0),
		make_index(offset + 6, p_type, 0),
 	];

	let v0 = (w * 0.5,  h * 0.25);
	let v1 = (w * 0.25, h * 0.75);
	let v2 = (w * 0.75, h * 0.75);

	p.add(0).write_unaligned(v0.0);
	p.add(1).write_unaligned(v0.1);
	p.add(2).cast::<u32>().write_unaligned(0xFF0000FF); // v0.c (1.0, 0.0, 0.0, 1.0)
	p.add(3).write_unaligned(v1.0);
	p.add(4).write_unaligned(v1.1);
	p.add(5).cast::<u32>().write_unaligned(0xFF00FF00); // v1.c (0.0, 1.0, 0.0, 1.0)
	p.add(6).write_unaligned(v2.0);
	p.add(7).write_unaligned(v2.1);
	p.add(8).cast::<u32>().write_unaligned(0xFFFF0000); // v2.c (0.0, 0.0, 1.0, 1.0)
	offset += 9;

	for (offset, index) in tri_indices.into_iter().enumerate() {
		i.add(offset).write_unaligned(index);
		indices += 1;
	}

	indices
}

fn ortho_projection(w: f32, h: f32) -> [[f32; 4]; 4] {
	let l = 0.0;
	let r = l + w;
	let t = 0.0;
	let b = t + h;
	let n = 0.0;
	let f = 1.0;
	let proj = [
		[
			2.0 / (r - l),
			0.0,
			0.0,
			0.0,
		],
		[
			0.0,
			2.0 / (b - t),
			0.0,
			0.0,
		],
		[
			0.0,
			0.0,
			1.0 / (n - f),
			0.0,
		],
		[
			-(r + l) / (r - l),
			-(b + t) / (b - t),
			n / (n - f),
			1.0,
		],
	];

	proj
}

unsafe fn create_instance(entry: &Entry) -> Result<ash::Instance> {
	// TODO: Enable validation layer (`VK_LAYER_KHRONOS_validation`).
	// and setup the debug callback to print messages.

	let app_info = vk::ApplicationInfo {
		api_version: vk::make_api_version(0, 1, 0, 0),
		..Default::default()
	};

	let instance_extensions = [
		khr::Surface::name().as_ptr(),
		khr::Win32Surface::name().as_ptr(),
		ext::DebugUtils::name().as_ptr(),
	];
	let layers = [
		// TODO: Make it a debug/cmd-line flag only.
		CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0").as_ptr(),
	];

	let create_info = vk::InstanceCreateInfo::builder()
		.application_info(&app_info)
		.enabled_extension_names(&instance_extensions)
		.enabled_layer_names(&layers);

	let instance = entry.create_instance(&create_info, None)?;

	Ok(instance)
}

struct Pipeline {
	handle: vk::Pipeline,
	layout: vk::PipelineLayout,
}

unsafe fn pick_physical_device_and_queue_family(
	instance: &ash::Instance,
	khr_surface: &khr::Surface,
	surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32)> {
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

	Ok((physical_device, queue_family))
}

unsafe fn create_device_and_queue(
	instance: &ash::Instance,
	physical_device: vk::PhysicalDevice,
	queue_family: u32,
) -> Result<(ash::Device, vk::Queue)> {
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

	Ok((device, queue))
}

unsafe fn create_tri_pipeline(
	device: &ash::Device,
	render_pass: vk::RenderPass,
	descriptor_set_layout: vk::DescriptorSetLayout,
) -> Result<Pipeline> {
	let vs_shader_spv = read_spv(Path::new("shaders/tri.vert.spv"))?;
	let shader_create_info = vk::ShaderModuleCreateInfo::builder()
		.code(&vs_shader_spv);
	let vs_shader = device.create_shader_module(&shader_create_info, None)?;

	let fs_shader_spv = read_spv(Path::new("shaders/tri.frag.spv"))?;
	let shader_create_info = vk::ShaderModuleCreateInfo::builder()
		.code(&fs_shader_spv);
	let fs_shader = device.create_shader_module(&shader_create_info, None)?;

	let pipeline_cache = vk::PipelineCache::null(); // TODO:

	let shader_main_name = CStr::from_bytes_with_nul_unchecked(b"main\0");
	let vs_stage_create_info = vk::PipelineShaderStageCreateInfo::builder()
		.stage(vk::ShaderStageFlags::VERTEX)
		.module(vs_shader)
		.name(&shader_main_name);
	let fs_stage_create_info = vk::PipelineShaderStageCreateInfo::builder()
		.stage(vk::ShaderStageFlags::FRAGMENT)
		.module(fs_shader)
		.name(&shader_main_name);

	let stages = [
		*vs_stage_create_info,
		*fs_stage_create_info,
	];

	let vertex_input_state_create_info = vk::PipelineVertexInputStateCreateInfo::default();

	let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
		.topology(vk::PrimitiveTopology::TRIANGLE_LIST);

	let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
		.viewport_count(1)
		.scissor_count(1);

	let rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
		.line_width(1.0);

	let multisample_state = vk::PipelineMultisampleStateCreateInfo::builder()
		.rasterization_samples(vk::SampleCountFlags::TYPE_1);

	let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default();

	let color_blend_state_attachment = vk::PipelineColorBlendAttachmentState::builder()
		.color_write_mask(vk::ColorComponentFlags::RGBA);
	let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
		.attachments(slice::from_ref(&color_blend_state_attachment));

	let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
	let dynamic_states = vk::PipelineDynamicStateCreateInfo::builder()
		.dynamic_states(&dynamic_states);

	let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::builder()
		.set_layouts(slice::from_ref(&descriptor_set_layout));
	let pipeline_layout = device.create_pipeline_layout(&pipeline_layout_create_info, None)?;

	let gfx_pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
		.stages(&stages)
		.vertex_input_state(&vertex_input_state_create_info)
		.input_assembly_state(&input_assembly_state)
		.viewport_state(&viewport_state)
		.rasterization_state(&rasterization_state)
		.multisample_state(&multisample_state)
		.depth_stencil_state(&depth_stencil_state)
		.color_blend_state(&color_blend_state)
		.dynamic_state(&dynamic_states)
		.layout(pipeline_layout)
		.render_pass(render_pass);

	let pipelines = device
		.create_graphics_pipelines(pipeline_cache, slice::from_ref(&gfx_pipeline_create_info), None)
		.expect("Failed to create a graphics pipeline.");

	device.destroy_shader_module(fs_shader, None);
	device.destroy_shader_module(vs_shader, None);

	let pipeline = Pipeline {
		handle: pipelines[0],
		layout: pipeline_layout,
	};

	Ok(pipeline)
}

unsafe fn register_window_class(hinstance: HINSTANCE) -> ATOM {
	let mut wc = WNDCLASSA::default();
	wc.lpfnWndProc = Some(window_procedure);
	wc.hInstance = hinstance;
	wc.lpszClassName = WINDOW_CLASS_NAME.as_ptr();
	wc.hCursor = LoadCursorA(ptr::null_mut(), IDC_ARROW);

	let atom = RegisterClassA(&wc);
	if atom == 0 {
		let last_error = GetLastError();
		panic!("Failed to register the window class, error code = {last_error}");
	}

	atom
}

unsafe fn create_window(hinstance: HINSTANCE, width: u32, height: u32) -> HWND {
	let window_name = CStr::from_bytes_with_nul_unchecked(b"Prima!\0");
	let hwnd = CreateWindowExA(
			0,
			WINDOW_CLASS_NAME.as_ptr(),
			window_name.as_ptr(),
			WS_OVERLAPPEDWINDOW,
			// TODO: Center the window or load/save the last position.
			CW_USEDEFAULT,
			CW_USEDEFAULT,
			// TODO: Deal with scaling/HI-DPI & window title.
			width as i32,
			height as i32,
			ptr::null_mut(),
			ptr::null_mut(),
			hinstance,
			ptr::null_mut(),
	);

	if hwnd.is_null() {
		panic!("Failed to create a window.");
	}

	hwnd
}

fn get_window_client_size(hwnd: HWND) -> (u32, u32) {
	let mut client_rect = RECT::default();
	let err = unsafe { GetClientRect(hwnd, &mut client_rect) };
	if err != 1 {
		panic!("Failed to get window client rect.");
	}

	(
		(client_rect.right - client_rect.left) as u32,
		(client_rect.bottom - client_rect.top) as u32,
	)
}

unsafe fn image_barrier(
	image: &vk::Image,
	src_access_mask: vk::AccessFlags,
	dst_access_mask: vk::AccessFlags,
	old_layout: vk::ImageLayout,
	new_layout: vk::ImageLayout,
) -> vk::ImageMemoryBarrier {
	let range = vk::ImageSubresourceRange::builder()
		.aspect_mask(vk::ImageAspectFlags::COLOR)
		.level_count(vk::REMAINING_MIP_LEVELS)
		.layer_count(vk::REMAINING_ARRAY_LAYERS); // Afair, those are not fully supported on Android.

	let barrier = vk::ImageMemoryBarrier::builder()
		.src_access_mask(src_access_mask)
		.dst_access_mask(dst_access_mask)
		.old_layout(old_layout)
		.new_layout(new_layout)
		.src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
		.dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
		.image(*image)
		.subresource_range(*range);

	*barrier
}

unsafe extern "system" fn vulkan_debug_message_callback(
	message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
	message_types: vk::DebugUtilsMessageTypeFlagsEXT,
	p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
	p_user_data: *mut c_void,
) -> vk::Bool32 {
	eprintln!("{} [{:?}] Validation issue:\n{:#?}", message_severity.as_raw(), message_types, *p_callback_data);
	vk::FALSE
}

fn read_spv(path: &Path) -> Result<Vec<u32>> {
	let mut f = fs::File::open(path)?;
	let len = f.metadata()?.len() as usize;
	assert!(len % 4 == 0);
	let words = len / 4;

	let mut spv = vec![0; words];
	f.read_exact(unsafe { slice::from_raw_parts_mut(spv.as_mut_ptr().cast::<u8>(), len) })?;

	assert!(spv[0] == 0x0723_0203);

	Ok(spv)
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
		WM_CLOSE | WM_KEYDOWN if wParam == VK_ESCAPE => drop(DestroyWindow(hwnd)),
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

	pub const PM_REMOVE:  u32 = 0x0001;

	pub const WM_DESTROY: u32 = 0x0002;
	pub const WM_CLOSE:   u32 = 0x0010;
	pub const WM_QUIT:    u32 = 0x0012;
	pub const WM_KEYDOWN: u32 = 0x0100;

	pub const VK_ESCAPE: usize = 0x1B;

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

		pub fn PeekMessageA(
			lpMsg: *const MSG,
			hWnd: HWND,
			wMsgFilterMin: UINT,
			wMsgFilterMax: UINT,
			wRemoveMsg: UINT,
		) -> BOOL;

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
